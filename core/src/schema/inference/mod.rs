//! # TODO
//! - Put the numerical content upcast logic in the corresponding function-style macro
#![allow(clippy::assertions_on_result_states)]
use anyhow::Result;
use serde_json::{Map, Number, Value};

use std::collections::HashSet;
use std::fmt::Display;

pub mod value;
pub use value::ValueMergeStrategy;

use super::{
    number_content, ArrayContent, BoolContent, Categorical, CategoricalType, ChronoValueFormatter,
    Content, DateTimeContent, Id, NumberContent, NumberKindExt, ObjectContent, OneOfContent,
    RangeStep, StringContent, ValueKindExt,
};
use crate::graph::prelude::content::number_content::{I8, I16, I32, I64};
use crate::schema::UniqueContent;
use num::Zero;

pub trait MergeStrategy<M, C>: std::fmt::Display {
    fn try_merge(self, master: &mut M, candidate: &C) -> Result<()>;
}

#[derive(Clone, Copy)]
pub struct OptionalMergeStrategy;

impl std::fmt::Display for OptionalMergeStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OptionalMergeStrategy")
    }
}

impl MergeStrategy<Content, Value> for OptionalMergeStrategy {
    fn try_merge(self, master: &mut Content, candidate: &Value) -> Result<()> {
        match (master, candidate) {
            // Logical nodes go first
            (Content::SameAs(_), _) => {
                // Nothing can happen here because this is not a visitor pattern
                Ok(())
            }
            (Content::OneOf(one_of_content), candidate) => {
                Self.try_merge(one_of_content, candidate)
            }
            (Content::Unique(unique_content), candidate) => {
                Self.try_merge(unique_content, candidate)
            }
            // Non-logical nodes go after
            (Content::Object(master_obj), Value::Object(candidate_obj)) => {
                Self.try_merge(master_obj, candidate_obj)
            }
            (Content::Array(ArrayContent { content, length }), Value::Array(values)) => {
                Self.try_merge(length.as_mut(), &Value::from(values.len()))?;
                values
                    .iter()
                    .try_for_each(|value| Self.try_merge(content.as_mut(), value))
            }
            (Content::String(string_content), Value::String(string)) => {
                Self.try_merge(string_content, string)
            }
            (Content::DateTime(date_time_content), Value::String(string)) => {
                Self.try_merge(date_time_content, string)
            }
            (Content::Number(number_content), Value::Number(number)) => {
                Self.try_merge(number_content, number)
            }
            (Content::Bool(bool_content), Value::Bool(boolean)) => {
                Self.try_merge(bool_content, boolean)
            }
            (Content::Null(_), Value::Null) => Ok(()),
            (master, candidate) => Err(failed!(
                target: Release,
                "cannot merge a node of type '{}' with a value of type '{}'",
                master.kind(),
                candidate.kind()
            )),
        }
    }
}

impl MergeStrategy<UniqueContent, Value> for OptionalMergeStrategy {
    fn try_merge(self, master: &mut UniqueContent, candidate: &Value) -> Result<()> {
        Self.try_merge(&mut *master.content, candidate)
    }
}

impl MergeStrategy<OneOfContent, Value> for OptionalMergeStrategy {
    fn try_merge(self, master: &mut OneOfContent, candidate: &Value) -> Result<()> {
        master.insert_with(self, candidate);
        Ok(())
    }
}

impl MergeStrategy<BoolContent, bool> for OptionalMergeStrategy {
    fn try_merge(self, master: &mut BoolContent, value: &bool) -> Result<()> {
        match master {
            BoolContent::Categorical(boolean_categorical) => {
                boolean_categorical.push(*value);
                Ok(())
            }
            BoolContent::Constant(val) => {
                if val == value {
                    Ok(())
                } else {
                    Err(failed!(
                        target: Release,
                        "value mismatch at constant node: {} != {}",
                        val,
                        value
                    ))
                }
            }
            BoolContent::Frequency(_) => Ok(()),
        }
    }
}

impl MergeStrategy<StringContent, String> for OptionalMergeStrategy {
    fn try_merge(self, master: &mut StringContent, value: &String) -> Result<()> {
        match master {
            StringContent::Pattern(_) => Ok(()),
            StringContent::Categorical(string_categorical) => {
                string_categorical.push(value.clone());
                Ok(())
            }
            StringContent::Faker(_) => Ok(()),
            StringContent::Serialized(_) => Ok(()), // we can probably do better here
            StringContent::Uuid(_) => Ok(()),
            StringContent::Truncated(_) => Ok(()),
            StringContent::Sliced(_) => Ok(()),
            StringContent::Constant(_) => Ok(()),
            StringContent::Format(_) => Ok(()),
        }
    }
}

impl MergeStrategy<ObjectContent, Map<String, Value>> for OptionalMergeStrategy {
    fn try_merge(
        self,
        master: &mut ObjectContent,
        candidate_obj: &serde_json::Map<String, Value>,
    ) -> Result<()> {
        let master_keys: HashSet<_> = master
            .iter()
            .filter_map(|(key, value)| {
                if !value.is_null() {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        let candidate_keys: HashSet<_> = candidate_obj
            .iter()
            .filter_map(|(key, value)| {
                if !value.is_null() {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        for key in master_keys.symmetric_difference(&candidate_keys) {
            if let Some(field) = master.fields.remove(key) {
                master.fields.insert(key.clone(), field.into_nullable());
            } else {
                // SAFETY: if `key` is not in master then it is in candidate
                let candidate_field = candidate_obj.get(key).unwrap();
                let field = Content::from(candidate_field).into_nullable();
                master.fields.insert(key.clone(), field);
            }
        }

        for key in master_keys.intersection(&candidate_keys) {
            // SAFETY: `key` is in both `self` and `candidate_obj`
            let master_value = master.get_mut(key).unwrap();
            let candidate_value = candidate_obj.get(key).unwrap();
            Self.try_merge(master_value, candidate_value)?;
        }

        Ok(())
    }
}

impl MergeStrategy<DateTimeContent, String> for OptionalMergeStrategy {
    fn try_merge(self, master: &mut DateTimeContent, candidate: &String) -> Result<()> {
        let fmt = ChronoValueFormatter::new(&master.format);
        let candidate = fmt.parse(candidate.as_str())?;
        if let Some(begin) = master.begin.as_mut() {
            if *begin > candidate {
                *begin = candidate.clone();
            }
        } else {
            master.begin = Some(candidate.clone());
        }

        if let Some(end) = master.end.as_mut() {
            if *end < candidate {
                *end = candidate;
            }
        } else {
            master.end = Some(candidate);
        }

        Ok(())
    }
}

impl<N> MergeStrategy<RangeStep<N>, N> for OptionalMergeStrategy
where
    N: PartialOrd + Copy,
{
    fn try_merge(self, master: &mut RangeStep<N>, value: &N) -> Result<()> {
        let low = master.low.get_or_insert(*value);
        *low = if *value < *low { *value } else { *low };
        let high = master.high.get_or_insert(*value);
        *high = if *value > *high { *value } else { *high };
        Ok(())
    }
}

impl<N> MergeStrategy<Categorical<N>, N> for OptionalMergeStrategy
where
    N: Copy + CategoricalType,
{
    fn try_merge(self, master: &mut Categorical<N>, value: &N) -> Result<()> {
        master.push(*value);
        Ok(())
    }
}

impl<N> MergeStrategy<Id<N>, N> for OptionalMergeStrategy
where
    N: PartialOrd + Zero + Copy,
{
    fn try_merge(self, master: &mut Id<N>, candidate: &N) -> Result<()> {
        let lower_bound = master.start_at.unwrap_or_else(N::zero);
        if candidate < &lower_bound {
            *master = Id {
                start_at: Some(*candidate),
            }
        }
        Ok(())
    }
}

impl<N: PartialEq + Display> MergeStrategy<N, N> for OptionalMergeStrategy {
    fn try_merge(self, master: &mut N, candidate: &N) -> Result<()> {
        if *master == *candidate {
            Ok(())
        } else {
            Err(failed!(
                target: Release,
                "value mismatch:  {} != {}",
                master,
                candidate
            ))
        }
    }
}

impl MergeStrategy<number_content::U64, u64> for OptionalMergeStrategy {
    fn try_merge(self, master: &mut number_content::U64, candidate: &u64) -> Result<()> {
        match master {
            number_content::U64::Range(range) => self.try_merge(range, candidate),
            number_content::U64::Categorical(cat) => self.try_merge(cat, candidate),
            number_content::U64::Constant(cst) => self.try_merge(cst, candidate),
            number_content::U64::Id(id) => self.try_merge(id, candidate),
        }
    }
}

impl MergeStrategy<number_content::I64, i64> for OptionalMergeStrategy {
    fn try_merge(self, master: &mut number_content::I64, candidate: &i64) -> Result<()> {
        match master {
            number_content::I64::Range(range) => self.try_merge(range, candidate),
            number_content::I64::Categorical(cat) => self.try_merge(cat, candidate),
            number_content::I64::Constant(cst) => self.try_merge(cst, candidate),
            I64::Id(id) => self.try_merge(id, candidate),
        }
    }
}

impl MergeStrategy<number_content::F64, f64> for OptionalMergeStrategy {
    fn try_merge(self, master: &mut number_content::F64, candidate: &f64) -> Result<()> {
        match master {
            number_content::F64::Range(range) => self.try_merge(range, candidate),
            number_content::F64::Constant(cst) => self.try_merge(cst, candidate),
        }
    }
}

impl MergeStrategy<number_content::U32, u32> for OptionalMergeStrategy {
    fn try_merge(self, master: &mut number_content::U32, candidate: &u32) -> Result<()> {
        match master {
            number_content::U32::Range(range) => self.try_merge(range, candidate),
            number_content::U32::Categorical(cat) => self.try_merge(cat, candidate),
            number_content::U32::Constant(cst) => self.try_merge(cst, candidate),
            number_content::U32::Id(id) => self.try_merge(id, candidate),
        }
    }
}

impl MergeStrategy<number_content::I32, i32> for OptionalMergeStrategy {
    fn try_merge(self, master: &mut number_content::I32, candidate: &i32) -> Result<()> {
        match master {
            number_content::I32::Range(range) => self.try_merge(range, candidate),
            number_content::I32::Categorical(cat) => self.try_merge(cat, candidate),
            number_content::I32::Constant(cst) => self.try_merge(cst, candidate),
            I32::Id(id) => self.try_merge(id, candidate),
        }
    }
}

impl MergeStrategy<number_content::F32, f32> for OptionalMergeStrategy {
    fn try_merge(self, master: &mut number_content::F32, candidate: &f32) -> Result<()> {
        match master {
            number_content::F32::Range(range) => self.try_merge(range, candidate),
            number_content::F32::Constant(cst) => self.try_merge(cst, candidate),
        }
    }
}

impl MergeStrategy<number_content::I16, i16> for OptionalMergeStrategy {
    fn try_merge(self, master: &mut number_content::I16, candidate: &i16) -> Result<()> {
        match master {
            number_content::I16::Range(range) => self.try_merge(range, candidate),
            number_content::I16::Categorical(cat) => self.try_merge(cat, candidate),
            number_content::I16::Constant(cst) => self.try_merge(cst, candidate),
            I16::Id(id) => self.try_merge(id, candidate),
        }
    }
}

impl MergeStrategy<number_content::I8, i8> for OptionalMergeStrategy {
    fn try_merge(self, master: &mut number_content::I8, candidate: &i8) -> Result<()> {
        match master {
            number_content::I8::Range(range) => self.try_merge(range, candidate),
            number_content::I8::Categorical(cat) => self.try_merge(cat, candidate),
            number_content::I8::Constant(cst) => self.try_merge(cst, candidate),
            I8::Id(id) => self.try_merge(id, candidate),
        }
    }
}

impl MergeStrategy<NumberContent, Number> for OptionalMergeStrategy {
    fn try_merge(self, master: &mut NumberContent, value: &Number) -> Result<()> {
        match master {
            NumberContent::U64(u64_content) => {
                if let Some(n) = value.as_u64() {
                    self.try_merge(u64_content, &n)
                } else {
                    *master = u64_content.clone().upcast(value.kind())?;
                    self.try_merge(master, value)
                }
            }
            NumberContent::I64(i64_content) => {
                if let Some(n) = value.as_i64() {
                    self.try_merge(i64_content, &n)
                } else {
                    *master = i64_content.clone().upcast(value.kind())?;
                    self.try_merge(master, value)
                }
            }
            NumberContent::F64(f64_content) => {
                if let Some(n) = value.as_f64() {
                    self.try_merge(f64_content, &n)
                } else {
                    *master = f64_content.clone().upcast(value.kind())?;
                    self.try_merge(master, value)
                }
            }
            NumberContent::U32(u32_content) => {
                if let Some(n) = value.as_u64() {
                    self.try_merge(u32_content, &(n as u32))
                } else {
                    todo!()
                }
            }
            NumberContent::I32(i32_content) => {
                if let Some(n) = value.as_i64() {
                    self.try_merge(i32_content, &(n as i32))
                } else {
                    todo!()
                }
            }
            NumberContent::F32(f32_content) => {
                if let Some(n) = value.as_f64() {
                    self.try_merge(f32_content, &(n as f32))
                } else {
                    todo!()
                }
            }
            NumberContent::I8(i8_content) => {
                if let Some(n) = value.as_i64() {
                    self.try_merge(i8_content, &(n as i8))
                } else {
                    todo!()
                }
            }
            NumberContent::I16(i16_content) => {
                if let Some(n) = value.as_i64() {
                    self.try_merge(i16_content, &(n as i16))
                } else {
                    todo!()
                }
            }
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::Namespace;

    macro_rules! as_array {
        [$($ident:ident)*] => {
            Value::from(vec![$($ident.clone())*])
        }
    }

    #[test]
    fn test_merge_different_fields() {
        let user_no_last_name = json!({
            "user_id" : 123,
            "first_name" : "John",
            "address" : {
                "postcode": "abc123",
                "numbers": 5.0
            }
        });

        let user_no_address = json!({
            "user_id" : 123,
            "first_name" : "John",
            "last_name": "Smith"
        });

        let user_no_last_name_as_array = as_array![user_no_last_name];
        let user_no_address_as_array = as_array![user_no_address];

        let collection_name = "users".to_string();
        let mut ns = Namespace::default();
        ns.put_collection_from_json(collection_name.clone(), &user_no_last_name)
            .unwrap();
        assert!(ns
            .accepts(&collection_name, &user_no_last_name_as_array)
            .is_ok());
        assert!(ns
            .accepts(&collection_name, &user_no_address_as_array)
            .is_err());
        ns.try_update(
            OptionalMergeStrategy,
            &collection_name,
            &user_no_address_as_array,
        )
        .unwrap();
        assert!(ns
            .accepts(&collection_name, &user_no_last_name_as_array)
            .is_ok());
        assert!(ns
            .accepts(&collection_name, &user_no_address_as_array)
            .is_ok());
    }

    #[test]
    fn test_merge_twice() {
        let user_no_last_name = json!({
            "user_id" : 123,
            "first_name" : "John",
            "address" : {
                "postcode": "abc123",
                "numbers": 5.0
            }
        });

        let user_no_address = json!({
                "user_id" : 123,
                "first_name" : "John",
                "last_name": "Smith"
        });

        let user_no_address_as_array = as_array![user_no_address];
        let user_no_last_name_as_array = as_array![user_no_last_name];

        let collection_name = "users".to_string();
        let mut ns = Namespace::default();
        ns.put_collection_from_json(collection_name.clone(), &user_no_last_name)
            .unwrap();
        ns.try_update(
            OptionalMergeStrategy,
            &collection_name,
            &user_no_address_as_array,
        )
        .unwrap();
        ns.try_update(
            OptionalMergeStrategy,
            &collection_name,
            &user_no_address_as_array,
        )
        .unwrap();
        assert!(ns
            .accepts(&collection_name, &user_no_last_name_as_array)
            .is_ok());
        assert!(ns
            .accepts(&collection_name, &user_no_address_as_array)
            .is_ok());
    }

    #[test]
    fn test_merge_different_fields_invalid_optional_field() {
        let user_no_last_name = json!({
            "user_id" : 123,
            "first_name" : "John",
            "address" : {
                "postcode": "abc123",
                "numbers": 5.0
            }
        });

        let user_no_address = json!({
            "user_id" : 123,
            "first_name" : "John",
            "last_name": "Smith"
        });

        let user_malformed_address = json!({
            "user_id" : 123,
            "first_name" : "John",
            "last_name": "Smith",
            "address" : {
                "bad_fields": "abc123",
            }
        });

        let user_no_address_as_array = as_array![user_no_address];
        let user_no_last_name_as_array = as_array![user_no_last_name];

        let collection_name = "users".to_string();
        let mut ns = Namespace::default();
        ns.put_collection_from_json(collection_name.clone(), &user_no_last_name)
            .unwrap();
        ns.try_update(
            OptionalMergeStrategy,
            &collection_name,
            &user_no_address_as_array,
        )
        .unwrap();
        assert!(ns
            .accepts(&collection_name, &user_no_last_name_as_array)
            .is_ok());
        assert!(ns
            .accepts(&collection_name, &user_no_address_as_array)
            .is_ok());
        assert!(ns
            .accepts(&collection_name, &as_array![user_malformed_address])
            .is_err());
    }

    #[test]
    fn merge_numbers() {
        let mut master: NumberContent = serde_json::from_value(json!({
            "subtype": "u64",
            "range": {
            "low": 0,
            "high": 10,
            "step": 1
            }
        }))
        .unwrap();
        let error_margin = f64::EPSILON;

        OptionalMergeStrategy
            .try_merge(&mut master, &"15".parse().unwrap())
            .unwrap();

        match master {
            NumberContent::U64(number_content::U64::Range(RangeStep {
                low, high, step, ..
            })) => {
                assert_eq!(low, Some(0));
                assert_eq!(high, Some(15));
                assert_eq!(step, Some(1));
            }
            _ => unreachable!(),
        }

        OptionalMergeStrategy
            .try_merge(&mut master, &"-10".parse().unwrap())
            .unwrap();
        OptionalMergeStrategy
            .try_merge(&mut master, &"20".parse().unwrap())
            .unwrap();

        match master {
            NumberContent::I64(number_content::I64::Range(RangeStep {
                low, high, step, ..
            })) => {
                assert_eq!(low, Some(-10));
                assert_eq!(high, Some(20));
                assert_eq!(step, Some(1));
            }
            _ => unreachable!(),
        }

        OptionalMergeStrategy
            .try_merge(&mut master, &"-13.6".parse().unwrap())
            .unwrap();
        OptionalMergeStrategy
            .try_merge(&mut master, &"20.6".parse().unwrap())
            .unwrap();

        match master {
            NumberContent::F64(number_content::F64::Range(RangeStep {
                low, high, step, ..
            })) => {
                assert!((low.unwrap() - -13.6).abs() < error_margin);
                assert!((high.unwrap() - 20.6).abs() < error_margin);
                assert!((step.unwrap() - 1.).abs() < error_margin);
            }
            _ => unreachable!(),
        }
    }
}
