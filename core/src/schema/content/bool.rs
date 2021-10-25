use super::prelude::*;

use super::Categorical;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum BoolContent {
    Frequency(f64),
    Constant(bool),
    Categorical(Categorical<bool>),
}

impl BoolContent {
    pub fn kind(&self) -> String {
        match self {
            Self::Frequency(_) => "frequency".to_string(),
            Self::Constant(_) => "constant".to_string(),
            Self::Categorical(_) => "categorical".to_string(),
        }
    }
}

impl Default for BoolContent {
    fn default() -> Self {
        Self::Frequency(0.5)
    }
}

impl Compile for BoolContent {
    fn compile<'a, C: Compiler<'a>>(&'a self, _compiler: C) -> Result<Graph> {
        let random_bool = match self {
            BoolContent::Frequency(p) => {
                let distr = Bernoulli::new(*p).map_err(|err| {
                    failed!(target: Release, "invalid frequency: p = '{}'", p).context(err)
                })?;
                RandomBool::Bernoulli(Random::new_with(distr))
            }
            BoolContent::Constant(value) => RandomBool::Constant(Yield::wrap(*value)),
            BoolContent::Categorical(categorical_content) => {
                RandomBool::Categorical(Random::new_with(categorical_content.clone()))
            }
        };
        Ok(Graph::Bool(random_bool.into()))
    }
}
