use super::prelude::*;
use crate::graph::prelude::content::number::number_content::U64;
use crate::graph::prelude::VariantContent;
use crate::schema::{number_content, NumberContent, RangeStep};
use serde::de;
use std::fmt;

#[derive(Debug, Serialize, Clone, PartialEq, Hash)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct ArrayContent {
    #[serde(default)]
    pub length: Box<Content>,
    pub content: Box<Content>,
}

lazy_static! {
    static ref NULL_VARIANT: VariantContent =
        VariantContent::new(Content::Null(prelude::NullContent {}));
}

impl<'de> Deserialize<'de> for ArrayContent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Length,
            Content,
        }

        struct ArrayVisitor;

        impl<'de> Visitor<'de> for ArrayVisitor {
            type Value = ArrayContent;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a object with a 'content' and 'length' value")
            }

            fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                let mut length = None;
                let mut content = None;

                while let Some(key) = access.next_key()? {
                    match key {
                        Field::Length => {
                            if length.is_some() {
                                return Err(de::Error::duplicate_field("length"));
                            }

                            length = Some(access.next_value()?);
                        }
                        Field::Content => {
                            if content.is_some() {
                                return Err(de::Error::duplicate_field("content"));
                            }

                            content = Some(access.next_value()?);
                        }
                    }
                }

                let length = length.ok_or_else(|| de::Error::missing_field("length"))?;
                let content = content.ok_or_else(|| de::Error::missing_field("content"))?;

                match length {
                    // Default for positive constants
                    Content::Number(NumberContent::U32(number_content::U32::Constant(_))) => { }
                    Content::Number(NumberContent::U64(number_content::U64::Range(r))) => {
                        if r.high.is_none() {
                            return Err(A::Error::custom(
                                "missing high value for array length range"
                            ));
                        }
                    }
                    // Default for ranges
                    Content::Number(NumberContent::U32(number_content::U32::Range(r))) => {
                        if r.high.is_none() {
                            return Err(A::Error::custom(
                                "missing high value for array length range"
                            ));
                        }
                    }
                    Content::Number(NumberContent::U64(_)) => {},
                    // Default for negative numbers
                    Content::Number(NumberContent::I64(number_content::I64::Constant(n))) => {
                        is_positive(n).map_err(A::Error::custom)?
                    }
                    Content::Number(NumberContent::I32(number_content::I32::Range(r))) => {
                        if r.high.is_none() {
                            return Err(A::Error::custom(
                                "missing high value for array length range"
                            ));
                        }

                        is_positive(
                            r.low .unwrap_or_default() .into(),
                        ).map_err(A::Error::custom)?
                    },
                    Content::Number(NumberContent::I16(number_content::I16::Range(r))) => {
                        if r.high.is_none() {
                            return Err(A::Error::custom("missing high value for array length range"));
                        }

                        is_positive(
                            r.low .unwrap_or_default() .into(),
                        ).map_err(A::Error::custom)?
                    },
                    Content::Number(NumberContent::I8(number_content::I8::Range(r))) => {
                        if r.high.is_none() {
                            return Err(A::Error::custom("missing high value for array length range"));
                        }

                        is_positive(
                            r.low .unwrap_or_default() .into(),
                        ).map_err(A::Error::custom)?
                    },
                    Content::SameAs(_) => {},
                    Content::Null(_) => return Err(de::Error::custom("array length is missing. Try adding '\"length\": [number]' to the array type where '[number]' is a positive integer")),
                    Content::Empty(_) => return Err(de::Error::custom("array length is not a constant or number type. Try replacing the '\"length\": {}' with '\"length\": [number]' where '[number]' is a positive integer")),
                    Content::OneOf(ref one) => if one.variants.iter().any(|variant| variant == &*NULL_VARIANT) {
                        return Err(de::Error::custom("cannot use 'one_of' with a 'null' variant nor '\"optional\": true' in array length"));
                    },
                    _ => {
                        return Err(de::Error::custom(
                            format!(
                                "cannot use {length} as an array length",
                            )
                        ));
                    }
                }

                if let Content::Empty(_) = content {
                    return Err(de::Error::custom("array content is missing. Try replacing the '\"content\": {}' with '\"content\": { \"type\": \"object\" }'"));
                }

                Ok(ArrayContent {
                    length: Box::new(length),
                    content: Box::new(content),
                })
            }
        }

        const FIELDS: &[&str] = &["length", "content"];
        deserializer.deserialize_struct("ArrayContent", FIELDS, ArrayVisitor)
    }
}

impl ArrayContent {
    pub fn from_content_default_length(content: Content) -> Self {
        Self {
            length: Box::new(Content::Number(NumberContent::U64(U64::Range(
                RangeStep::new(1, 2, 1),
            )))),
            content: Box::new(content),
        }
    }
}

impl Compile for ArrayContent {
    fn compile<'a, C: Compiler<'a>>(&'a self, mut compiler: C) -> Result<Graph> {
        let length = compiler.build("length", self.length.as_ref())?.into_size();
        let content = compiler.build("content", &self.content)?;
        Ok(Graph::Array(ArrayNode::new_with(length, content)))
    }
}

fn is_positive(i: i64) -> Result<()> {
    if i.is_negative() {
        return Err(anyhow!("cannot have a negative array length"));
    }

    Ok(())
}

impl Find<Content> for ArrayContent {
    fn project<I, R>(&self, mut reference: Peekable<I>) -> Result<&Content>
    where
        I: Iterator<Item = R>,
        R: AsRef<str>,
    {
        match reference.next() {
            Some(next) if next.as_ref() == "content" => self.content.project(reference),
            Some(next) if next.as_ref() == "length" => self.length.project(reference),
            otherwise => Err(failed!(
                target: Release,
                "expected 'content', found {}",
                otherwise.map_or_else(|| "nothing".to_string(), |s| format!("'{}'", s.as_ref()))
            )),
        }
    }

    fn project_mut<I, R>(&mut self, mut reference: Peekable<I>) -> Result<&mut Content>
    where
        I: Iterator<Item = R>,
        R: AsRef<str>,
    {
        match reference.next() {
            Some(next) if next.as_ref() == "content" => self.content.project_mut(reference),
            Some(next) if next.as_ref() == "length" => self.length.project_mut(reference),
            otherwise => Err(failed!(
                target: Release,
                "expected 'content', found {}",
                otherwise.map_or_else(|| "nothing".to_string(), |s| format!("'{}'", s.as_ref()))
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::schema::content::Content;
    use paste::paste;

    macro_rules! supported_length_tests {
        ($($name:ident: {$($schema:tt)*},)*) => {
        $(paste!{
            #[test]
            fn [<supported_length_ $name>]() {
                let _schema: Content = schema!({$($schema)*});
            }
        })*
        }
    }

    supported_length_tests! {
        default: {
            "type": "array",
            "length": 1,
            "content": {
                "type": "object"
            }
        },
        default_constant: {
            "type": "array",
            "length": {
                "type": "number",
                "constant": 8
            },
            "content": {
                "type": "object"
            }
        },
        u32_constant: {
            "type": "array",
            "length": {
                "type": "number",
                "subtype": "u32",
                "constant": 8
            },
            "content": {
                "type": "object"
            }
        },
        u64_constant: {
            "type": "array",
            "length": {
                "type": "number",
                "subtype": "u64",
                "constant": 3
            },
            "content": {
                "type": "object"
            }
        },
        default_range: {
            "type": "array",
            "length": {
                "type": "number",
                "range": {
                    "low": 5,
                    "high": 150
                }
            },
            "content": {
                "type": "object"
            }
        },
        u64_range: {
            "type": "array",
            "length": {
                "type": "number",
                "subtype": "u64",
                "range": {
                    "low": 7,
                    "high": 8
                }
            },
            "content": {
                "type": "object"
            }
        },
    }

    macro_rules! negative_length_tests {
        ($($name:ident: {$($schema:tt)*},)*) => {
        $(paste!{
            #[test]
            #[should_panic(expected = "cannot have a negative array length")]
            fn [<negative_length_ $name>]() {
                let _schema: Content = schema!({$($schema)*});
            }
        })*
        }
    }

    negative_length_tests! {
        default: {
            "type": "array",
            "length": -1,
            "content": {
                "type": "object"
            }
        },
        default_range: {
            "type": "array",
            "length": {
                "type": "number",
                "range": {
                    "low": -5,
                    "high": 40
                }
            },
            "content": {
                "type": "object"
            }
        },
    }

    macro_rules! unsupported_length_tests {
        ($($name:ident: {$($schema:tt)*},)*) => {
        $(paste!{
            #[test]
            #[should_panic(expected = "cannot use")]
            fn [<unsupported_length_ $name>]() {
                let _schema: Content = schema!({$($schema)*});
            }
        })*
        }
    }

    unsupported_length_tests! {
        i32_constant: {
            "type": "array",
            "length": {
                "type": "number",
                "subtype": "i32",
                "constant": 10
            },
            "content": {
                "type": "object"
            }
        },
        i64_range: {
            "type": "array",
            "length": {
                "type": "number",
                "subtype": "i64",
                "range": {}
            },
            "content": {
                "type": "object"
            }
        },
        default_float_constant: {
            "type": "array",
            "length": {
                "type": "number",
                "constant": -5.0
            },
            "content": {
                "type": "object"
            }
        },
        f64_constant: {
            "type": "array",
            "length": {
                "type": "number",
                "subtype": "f64",
                "constant": -5.0
            },
            "content": {
                "type": "object"
            }
        },
        f64_range: {
            "type": "array",
            "length": {
                "type": "number",
                "subtype": "f64",
                "range": {}
            },
            "content": {
                "type": "object"
            }
        },
        f32_constant: {
            "type": "array",
            "length": {
                "type": "number",
                "subtype": "f32",
                "constant": 3.0
            },
            "content": {
                "type": "object"
            }
        },
        f32_range: {
            "type": "array",
            "length": {
                "type": "number",
                "subtype": "f32",
                "range": {}
            },
            "content": {
                "type": "object"
            }
        },
    }

    macro_rules! missing_high_length_tests {
        ($($name:ident: {$($schema:tt)*},)*) => {
        $(paste!{
            #[test]
            #[should_panic(expected = "missing high value for array length")]
            fn [<missing_high_length_ $name>]() {
                let _schema: Content = schema!({$($schema)*});
            }
        })*
        }
    }

    missing_high_length_tests! {
        u64_default: {
            "type": "array",
            "length": {
                "type": "number",
                "subtype": "u64",
            },
            "content": {
                "type": "object"
            }
        },
        u64_range: {
            "type": "array",
            "length": {
                "type": "number",
                "subtype": "u64",
                "range": {}
            },
            "content": {
                "type": "object"
            }
        },
    }

    #[test]
    #[should_panic(expected = "missing field `length`")]
    fn missing_array_length() {
        let _schema: Content = schema!({
            "type": "array",
            "content": {
                "type": "object"
            }
        });
    }

    #[test]
    #[should_panic(
        expected = "cannot use 'one_of' with a 'null' variant nor '\\\"optional\\\": true' in array length"
    )]
    fn optional_array_length() {
        let _schema: Content = schema!({
            "type": "array",
            "length": {
                "type": "number",
                "constant": 5,
                "optional": true
            },
            "content": {
                "type": "object"
            }
        });
    }

    #[test]
    #[should_panic(
        expected = "cannot use 'one_of' with a 'null' variant nor '\\\"optional\\\": true' in array length"
    )]
    fn one_of_null_array_length() {
        let _schema: Content = schema!({
            "type": "array",
            "length": {
                "type": "one_of",
                "variants": [
                    {
                        "type": "null",
                    },
                    {
                        "type": "number",
                        "constant": 5
                    }
                ]
            },
            "content": {
                "type": "object"
            }
        });
    }

    #[test]
    fn one_of_array_length() {
        let _schema: Content = schema!({
            "type": "array",
            "length": {
                "type": "one_of",
                "variants": [{
                    "type": "number",
                    "constant": 3
                }, {
                    "type": "number",
                    "constant": 5
                }]
            },
            "content": {
                "type": "object"
            }
        });
    }
}
