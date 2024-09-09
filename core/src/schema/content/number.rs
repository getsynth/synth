#![allow(clippy::derive_partial_eq_without_eq)]
use super::prelude::*;
use std::hash::{Hash, Hasher};

use super::Categorical;

use crate::graph::number::{RandomF32, RandomI8, RandomI16, RandomI32, RandomU32};
use serde::{ser::Serializer, Serialize};

#[derive(Clone, Copy)]
pub enum NumberContentKind {
    U64,
    I64,
    F64,
}

impl NumberContentKind {
    #[must_use]
    pub fn upcast(self) -> Self {
        match self {
            Self::U64 => Self::I64,
            Self::I64 | Self::F64 => Self::F64,
        }
    }
}

pub trait NumberKindExt {
    fn kind(&self) -> NumberContentKind;
}

impl NumberKindExt for Number {
    fn kind(&self) -> NumberContentKind {
        if self.is_u64() {
            NumberContentKind::U64
        } else if self.is_i64() {
            NumberContentKind::I64
        } else if self.is_f64() {
            NumberContentKind::F64
        } else {
            unreachable!()
        }
    }
}

// Generate a snake case version of NumberContent variant
// name via `serde`. We use this hack because `serde` case
// conversion functions are internal
macro_rules! serde_convert_case {
    ($identifier:ident,$case:expr) => {{
        #[derive(Serialize)]
        #[serde(rename_all = $case)]
        enum SnakeCaseHelper {
            $identifier,
        }
        // Safety: since we derive `Serialize`, unwrap() shouldn't panic
        // for any identifier that doesn't brake `enum` compilation
        serde_json::to_value(&SnakeCaseHelper::$identifier).unwrap()
    }};
}

macro_rules! number_content {
    {
        $(
            $(#[$default:meta])?
            $ty:ty[$is:ident, $def:ident] as $as:ident {
                $(
                    $variant:ident($variant_ty:path),
                )*
            },
        )*
    } => {
        #[derive(Debug, Clone, PartialEq, Hash)]
        pub enum NumberContent {
            $($as(number_content::$as),)*
        }

        impl NumberContent {
            pub fn kind(&self) -> String {
                match self {
                    $($(Self::$as(number_content::$as::$variant(_)) => {
                        concat!(stringify!($as), "::", stringify!($variant)).to_string()
                    },)*)*
                }
            }

            $(
                pub fn $def() -> Self {
                    Self::$as(number_content::$as::Range(RangeStep::default()))
                }

                pub fn $is(&self) -> bool {
                    match self {
                        Self::$as(_) => true,
                        _ => false
                    }
                }
            )*
        }

        impl Serialize for NumberContent {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                match &self {
                    $(
                        NumberContent::$as(value) => {
                            let mut obj = serde_json::to_value(value).map_err(S::Error::custom)?;
                            let obj_map = obj.as_object_mut().ok_or(S::Error::custom("Object value expected"))?;
                            obj_map.insert("subtype".to_string(), serde_convert_case!($as, "snake_case"));
                            obj_map.serialize(serializer)
                        }
                    )*
                }
            }
        }

        impl<'de> Deserialize<'de> for NumberContent {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let mut v: serde_json::Value = Value::deserialize(deserializer)?;
                let as_object = v.as_object_mut().ok_or(D::Error::custom("Object value expected"))?;
                match as_object.remove("subtype") {
                    Some(subtype) => {
                        $(
                            if subtype == stringify!($ty) {
                                if as_object.is_empty() {
                                    Ok(Self::$def())
                                } else {
                                    let inner = number_content::$as::deserialize(v).map_err(D::Error::custom)?;
                                    Ok(NumberContent::$as(inner))
                                }
                            } else
                        )*
                            {
                                Err(D::Error::unknown_variant(format!("{:?}", subtype).as_str(), &[""]))
                            }
                    }
                    None => {
                        $( if let Ok(inner) = number_content::$as::deserialize(&v) {
                            Ok(NumberContent::$as(inner))
                        } else )* {
                            Err(D::Error::custom("Failed to infer numeric type from its value: try specifying the 'subtype' parameter"))
                        }
                    }
                }
            }
        }

        pub mod number_content {
            use super::{RangeStep, Categorical, NumberContent};
            use serde::{Serialize, Deserialize};

            $(
                #[derive(Debug, Serialize, Deserialize, Clone)]
                #[serde(rename_all = "snake_case")]
                #[serde(deny_unknown_fields)]
                $(#[$default])?
                pub enum $as {
                    $($variant($variant_ty),)*
                }

                $(
                    impl From<$variant_ty> for $as {
                        fn from(value: $variant_ty) -> Self {
                            Self::$variant(value)
                        }
                    }
                )*

                impl From<$as> for NumberContent {
                    fn from(value: $as) -> Self {
                        Self::$as(value)
                    }
                }
            )*
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default, Hash)]
#[serde(deny_unknown_fields)]
pub struct Id<N> {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_at: Option<N>,
}

impl NumberContent {
    pub fn accepts(&self, number: &Number) -> Result<()> {
        if self.is_u64() && number.is_u64()
            || self.is_i64() && number.is_i64()
            || self.is_f64() && number.is_f64()
        {
            Ok(())
        } else {
            // TODO: better error
            Err(failed!(target: Release, "numerical type mismatch"))
        }
    }

    pub fn try_transmute_to_id(self) -> Result<Self> {
        match self {
            NumberContent::U32(_) => Ok(Self::u32_default_id()),
            NumberContent::U64(_) => Ok(Self::u64_default_id()),
            NumberContent::I8(_) => Ok(Self::i8_default_id()),
            NumberContent::I16(_) => Ok(Self::i16_default_id()),
            NumberContent::I32(_) => Ok(Self::i32_default_id()),
            NumberContent::I64(_) => Ok(Self::i64_default_id()),
            NumberContent::F64(_) => bail!("could not transmute f64 into id"),
            NumberContent::F32(_) => bail!("could not transmute f32 into id"),
        }
    }

    pub fn u32_default_id() -> Self {
        NumberContent::U32(number_content::U32::Id(Id::default()))
    }

    pub fn u64_default_id() -> Self {
        NumberContent::U64(number_content::U64::Id(Id::default()))
    }

    pub fn i8_default_id() -> Self {
        NumberContent::I8(number_content::I8::Id(Id::default()))
    }

    pub fn i16_default_id() -> Self {
        NumberContent::I16(number_content::I16::Id(Id::default()))
    }

    pub fn i32_default_id() -> Self {
        NumberContent::I32(number_content::I32::Id(Id::default()))
    }

    pub fn i64_default_id() -> Self {
        NumberContent::I64(number_content::I64::Id(Id::default()))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct RangeStep<N> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low: Option<N>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high: Option<N>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<N>,
    #[serde(skip_serializing_if = "std::clone::Clone::clone")]
    pub include_low: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub include_high: bool,
}

impl<N> Default for RangeStep<N> {
    fn default() -> Self {
        Self {
            low: None,
            high: None,
            step: None,
            include_low: true,
            include_high: false,
        }
    }
}

impl<N> RangeStep<N> {
    pub fn new(low: N, high: N, step: N) -> Self {
        Self {
            low: Some(low),
            high: Some(high),
            step: Some(step),
            ..Default::default()
        }
    }

    fn bound(value: Option<&N>, inclusive: bool) -> std::ops::Bound<&N> {
        match value {
            Some(n) if inclusive => std::ops::Bound::Included(n),
            Some(n) => std::ops::Bound::Excluded(n),
            None => std::ops::Bound::Unbounded,
        }
    }

    fn cast<F, M>(self, f: F) -> RangeStep<M>
    where
        F: Fn(N) -> M,
    {
        self.try_cast::<_, _, std::convert::Infallible>(|value| Ok(f(value)))
            .unwrap()
    }

    fn try_cast<F, M, E>(self, f: F) -> Result<RangeStep<M>, E>
    where
        F: Fn(N) -> Result<M, E>,
    {
        Ok(RangeStep::<M> {
            low: self.low.map(&f).transpose()?,
            high: self.high.map(&f).transpose()?,
            step: self.step.map(&f).transpose()?,
            include_low: self.include_low,
            include_high: self.include_high,
        })
    }
}

impl<N: Copy> RangeStep<N> {
    pub fn step(&self) -> Option<N> {
        self.step.as_ref().cloned()
    }
}

impl<N> std::ops::RangeBounds<N> for RangeStep<N> {
    fn start_bound(&self) -> std::ops::Bound<&N> {
        RangeStep::bound(self.low.as_ref(), self.include_low)
    }

    fn end_bound(&self) -> std::ops::Bound<&N> {
        RangeStep::bound(self.high.as_ref(), self.include_high)
    }
}

macro_rules! derive_hash {
    (f32) => {
        impl Hash for RangeStep<f32> {
            derive_hash!(float);
        }
    };
    (f64) => {
        impl Hash for RangeStep<f64> {
            derive_hash!(float);
        }
    };
    (float) => {
            fn hash<H: Hasher>(&self, state: &mut H) {
                if let Some(low) = self.low {
                    low.to_bits().hash(state);
                }

                if let Some(high) = self.high {
                    high.to_bits().hash(state);
                }

                if let Some(step) = self.step {
                    step.to_bits().hash(state);
                }

                self.include_low.hash(state);
                self.include_high.hash(state);
            }
    };
    {$t:ty} => {
        impl Hash for RangeStep<$t> {
            fn hash<H: Hasher>(&self, state: &mut H) {
                if let Some(low) = self.low {
                    low.hash(state);
                }

                if let Some(high) = self.high {
                    high.hash(state);
                }

                if let Some(step) = self.step {
                    step.hash(state);
                }

                self.include_low.hash(state);
                self.include_high.hash(state);
            }
        }
    };
    {$($t:ident),*} => {
        $(derive_hash!{$t})*
    };
}

derive_hash!(i8, i16, i32, u32, i64, u64, f32, f64);

number_content!(
    #[derive(PartialEq, Hash)]
    u32[is_u32, default_u32_range] as U32 {
        Range(RangeStep<u32>),
        Categorical(Categorical<u32>),
        Constant(u32),
        Id(crate::schema::Id<u32>),
    },
    #[derive(PartialEq, Hash)]
    u64[is_u64, default_u64_range] as U64 {
        Range(RangeStep<u64>),
        Categorical(Categorical<u64>),
        Constant(u64),
        Id(crate::schema::Id<u64>),
    },
    #[derive(PartialEq, Hash)]
    i8[is_i8, default_i8_range] as I8 {
        Range(RangeStep<i8>),
        Categorical(Categorical<i8>),
        Constant(i8),
        Id(crate::schema::Id<i8>),
    },
    #[derive(PartialEq, Hash)]
    i16[is_i16, default_i16_range] as I16 {
        Range(RangeStep<i16>),
        Categorical(Categorical<i16>),
        Constant(i16),
        Id(crate::schema::Id<i16>),
    },
    #[derive(PartialEq, Hash)]
    i32[is_i32, default_i32_range] as I32 {
        Range(RangeStep<i32>),
        Categorical(Categorical<i32>),
        Constant(i32),
        Id(crate::schema::Id<i32>),
    },
    #[derive(PartialEq, Hash)]
    i64[is_i64, default_i64_range] as I64 {
        Range(RangeStep<i64>),
        Categorical(Categorical<i64>),
        Constant(i64),
        Id(crate::schema::Id<i64>),
    },
    f64[is_f64, default_f64_range] as F64 {
        Range(RangeStep<f64>),
        Constant(f64),
    },
    f32[is_f32, default_f32_range] as F32 {
        Range(RangeStep<f32>),
        Constant(f32),
    },
);

impl Compile for NumberContent {
    fn compile<'a, C: Compiler<'a>>(&'a self, _compiler: C) -> Result<Graph> {
        let number_node = match self {
            Self::U64(u64_content) => {
                let random_u64 = match u64_content {
                    number_content::U64::Range(range) => RandomU64::range(*range)?,
                    number_content::U64::Categorical(categorical_content) => {
                        RandomU64::categorical(categorical_content.clone())
                    }
                    number_content::U64::Constant(val) => RandomU64::constant(*val),
                    number_content::U64::Id(id) => {
                        let gen = Incrementing::new_at(id.start_at.unwrap_or(1));
                        RandomU64::incrementing(gen)
                    }
                };
                random_u64.into()
            }
            Self::I64(i64_content) => {
                let random_i64 = match i64_content {
                    number_content::I64::Range(range) => RandomI64::range(*range)?,
                    number_content::I64::Categorical(categorical_content) => {
                        RandomI64::categorical(categorical_content.clone())
                    }
                    number_content::I64::Constant(val) => RandomI64::constant(*val),
                    number_content::I64::Id(id) => {
                        RandomI64::incrementing(Incrementing::new_at(id.start_at.unwrap_or(1)))
                    }
                };
                random_i64.into()
            }
            Self::F64(f64_content) => {
                let random_f64 = match f64_content {
                    number_content::F64::Range(range) => RandomF64::range(*range)?,
                    number_content::F64::Constant(val) => RandomF64::constant(*val),
                };
                random_f64.into()
            }
            Self::U32(u32_content) => {
                let random_u32 = match u32_content {
                    number_content::U32::Range(range) => RandomU32::range(*range)?,
                    number_content::U32::Categorical(categorical_content) => {
                        RandomU32::categorical(categorical_content.clone())
                    }
                    number_content::U32::Constant(val) => RandomU32::constant(*val),
                    number_content::U32::Id(id) => {
                        RandomU32::incrementing(Incrementing::new_at(id.start_at.unwrap_or(1)))
                    }
                };
                random_u32.into()
            }
            Self::I32(i32_content) => {
                let random_i32 = match i32_content {
                    number_content::I32::Range(range) => RandomI32::range(*range)?,
                    number_content::I32::Categorical(categorical_content) => {
                        RandomI32::categorical(categorical_content.clone())
                    }
                    number_content::I32::Constant(val) => RandomI32::constant(*val),
                    number_content::I32::Id(id) => {
                        RandomI32::incrementing(Incrementing::new_at(id.start_at.unwrap_or(1)))
                    }
                };
                random_i32.into()
            }
            Self::F32(f32_content) => {
                let random_f32 = match f32_content {
                    number_content::F32::Range(range) => RandomF32::range(*range)?,
                    number_content::F32::Constant(val) => RandomF32::constant(*val),
                };
                random_f32.into()
            }
            Self::I8(i8_content) => {
                let random_i8 = match i8_content {
                    number_content::I8::Range(range) => RandomI8::range(*range)?,
                    number_content::I8::Categorical(categorical_content) => {
                        RandomI8::categorical(categorical_content.clone())
                    }
                    number_content::I8::Constant(val) => RandomI8::constant(*val),
                    number_content::I8::Id(id) => {
                        RandomI8::incrementing(Incrementing::new_at(id.start_at.unwrap_or(1)))
                    }
                };
                random_i8.into()
            }
            Self::I16(i16_content) => {
                let random_i16 = match i16_content {
                    number_content::I16::Range(range) => RandomI16::range(*range)?,
                    number_content::I16::Categorical(categorical_content) => {
                        RandomI16::categorical(categorical_content.clone())
                    }
                    number_content::I16::Constant(val) => RandomI16::constant(*val),
                    number_content::I16::Id(id) => {
                        RandomI16::incrementing(Incrementing::new_at(id.start_at.unwrap_or(1)))
                    }
                };
                random_i16.into()
            }
        };
        Ok(Graph::Number(number_node))
    }
}

impl RangeStep<u64> {
    pub fn upcast(self, to: NumberContentKind) -> Result<NumberContent> {
        match to {
            NumberContentKind::U64 => Ok(number_content::U64::Range(self).into()),
            NumberContentKind::I64 => {
                let cast = self.try_cast(i64::try_from)?;
                Ok(number_content::I64::Range(cast).into())
            }
            NumberContentKind::F64 => {
                let cast = self.cast(|value| value as f64);
                Ok(number_content::F64::Range(cast).into())
            }
        }
    }
}

impl Categorical<u64> {
    pub fn upcast(self, to: NumberContentKind) -> Result<NumberContent> {
        match to {
            NumberContentKind::U64 => Ok(number_content::U64::Categorical(self).into()),
            NumberContentKind::I64 => {
                let cast = Categorical {
                    seen: self
                        .seen
                        .into_iter()
                        .map(|(k, v)| {
                            i64::try_from(k)
                                .map(|k_cast| (k_cast, v))
                                .map_err(|err| err.into())
                        })
                        .collect::<Result<_>>()?,
                    total: self.total,
                };
                Ok(number_content::I64::Categorical(cast).into())
            }
            NumberContentKind::F64 => Err(
                failed!(target: Release, "cannot upcast categorical subtypes to accept floats; try changing this another numerical subtype manually"),
            ),
        }
    }
}

impl Id<i64> {
    pub fn upcast(self, to: NumberContentKind) -> Result<NumberContent> {
        match to {
            NumberContentKind::U64 => {
                let start_at = self.start_at.unwrap_or(1);
                if start_at < 0 {
                    Err(failed!(
                        target: Release,
                        "cannot cast id with negative start_at to u64"
                    ))
                } else {
                    Ok(number_content::U64::Id(Id {
                        start_at: Some(start_at as u64),
                    })
                    .into())
                }
            }
            NumberContentKind::I64 => Ok(number_content::I64::Id(self).into()),
            NumberContentKind::F64 => Err(failed!(target: Release, "cannot cast id f64")),
        }
    }
}

impl RangeStep<i64> {
    pub fn upcast(self, to: NumberContentKind) -> Result<NumberContent> {
        match to {
            NumberContentKind::U64 => Err(failed!(
                target: Release,
                "cannot downcast numerical subtypes"
            )),
            NumberContentKind::I64 => Ok(number_content::I64::Range(self).into()),
            NumberContentKind::F64 => {
                let cast = self.cast(|value| value as f64);
                Ok(number_content::F64::Range(cast).into())
            }
        }
    }
}

impl RangeStep<f64> {
    pub fn upcast(self, to: NumberContentKind) -> Result<NumberContent> {
        match to {
            NumberContentKind::U64 | NumberContentKind::I64 => Err(failed!(
                target: Release,
                "cannot downcast numerical subtypes"
            )),
            NumberContentKind::F64 => Ok(number_content::F64::Range(self).into()),
        }
    }
}

impl Categorical<i64> {
    pub fn upcast(self, to: NumberContentKind) -> Result<NumberContent> {
        match to {
            NumberContentKind::U64 => {
                Err(failed!(target: Release, "cannot downcast numerical subtypes"))
            }
            NumberContentKind::I64 => Ok(number_content::I64::Categorical(self).into()),
            NumberContentKind::F64 => Err(
                failed!(target: Release, "cannot upcast categorical subtypes to accept floats; try changing this another numerical subtype manually"),
            ),
        }
    }
}

impl number_content::U64 {
    pub fn upcast(self, to: NumberContentKind) -> Result<NumberContent> {
        match self {
            Self::Range(range) => range.upcast(to),
            Self::Categorical(cat) => cat.upcast(to),
            Self::Constant(val) => match to {
                NumberContentKind::U64 => Ok(self.into()),
                NumberContentKind::I64 => {
                    let cast = i64::try_from(val)?;
                    Ok(number_content::I64::Constant(cast).into())
                }
                NumberContentKind::F64 => {
                    let cast = val as f64;
                    Ok(number_content::F64::Constant(cast).into())
                }
            },
            Self::Id(_id) => Err(failed!(
                target: Release,
                "cannot upcast an id number subtype: only unsigned integers are supported"
            )),
        }
    }
}

impl number_content::I64 {
    pub fn upcast(self, to: NumberContentKind) -> Result<NumberContent> {
        match self {
            Self::Range(range) => range.upcast(to),
            Self::Categorical(cat) => cat.upcast(to),
            Self::Constant(val) => match to {
                NumberContentKind::U64 => Err(failed!(
                    target: Release,
                    "cannot downcast numerical subtypes"
                )),
                NumberContentKind::I64 => Ok(self.into()),
                NumberContentKind::F64 => {
                    let cast = val as f64;
                    Ok(number_content::F64::Constant(cast).into())
                }
            },
            Self::Id(id) => id.upcast(to),
        }
    }
}

impl number_content::F64 {
    pub fn upcast(self, to: NumberContentKind) -> Result<NumberContent> {
        match self {
            Self::Range(range) => range.upcast(to),
            Self::Constant(_) => match to {
                NumberContentKind::U64 => Err(failed!(
                    target: Release,
                    "cannot downcast numerical subtypes"
                )),
                NumberContentKind::I64 => Err(failed!(
                    target: Release,
                    "cannot downcast numerical subtypes"
                )),
                NumberContentKind::F64 => Ok(self.into()),
            },
        }
    }
}

impl Hash for number_content::F32 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Range(range) => range.hash(state),
            Self::Constant(constant) => constant.to_bits().hash(state),
        }
    }
}

impl PartialEq for number_content::F32 {
    fn eq(&self, other: &number_content::F32) -> bool {
        match self {
            Self::Range(range) => match other {
                Self::Range(o_range) => range == o_range,
                _ => false,
            },
            Self::Constant(constant) => match other {
                Self::Constant(o_constant) => constant == o_constant,
                _ => false,
            },
        }
    }
}

impl Hash for number_content::F64 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Range(range) => range.hash(state),
            Self::Constant(constant) => constant.to_bits().hash(state),
        }
    }
}

impl PartialEq for number_content::F64 {
    fn eq(&self, other: &number_content::F64) -> bool {
        match self {
            Self::Range(range) => match other {
                Self::Range(o_range) => range == o_range,
                _ => false,
            },
            Self::Constant(constant) => match other {
                Self::Constant(o_constant) => constant == o_constant,
                _ => false,
            },
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    use num::One;

    // TODO: f32 deserializes successfully to `inf` when OOR
    macro_rules! test_number_variants {
        ($($test:ident -> $name:literal $as:ident: $ty:ty $(,)?)+) => {
            $(
                #[test]
                fn $test() {
                    // Inferred
                    let number_content_as_json = json!({
                        "range": {
                            "low": <$ty>::MIN,
                            "high": <$ty>::MAX
                        }
                    });
                    let number_content: NumberContent = serde_json::from_value(number_content_as_json).unwrap();
                    assert_eq!(
                        number_content,
                        NumberContent::$as(number_content::$as::Range(RangeStep {
                            low: Some(<$ty>::MIN),
                            high: Some(<$ty>::MAX),
                            step: None,
                            ..Default::default()
                        }))
                    );

                    // Specified
                    let number_content_as_json = json!({
                        "subtype": $name,
                        "constant": <$ty>::one()
                    });
                    let number_content: NumberContent = serde_json::from_value(number_content_as_json).unwrap();
                    assert_eq!(
                        number_content,
                        NumberContent::$as(number_content::$as::Constant(<$ty>::one()))
                    );
                }
            )+
        }
    }

    test_number_variants!(
        test_u32 -> "u32" U32: u32,
        test_u64 -> "u64" U64: u64,
        test_i32 -> "i32" I32: i32,
        test_i64 -> "i64" I64: i64,
    );
}
