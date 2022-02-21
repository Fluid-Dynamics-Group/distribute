use derive_more::{Display, From};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

#[derive(From, Debug, Clone, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Requirements<T> {
    pub(crate) reqs: BTreeSet<Requirement>,
    #[serde(skip)]
    pub(crate) marker: std::marker::PhantomData<T>,
}

impl Requirements<NodeProvidedCaps> {
    pub(crate) fn can_accept_job(&self, job_reqs: &Requirements<JobRequiredCaps>) -> bool {
        self.reqs.is_superset(&job_reqs.reqs)
    }
}

impl<T> FromIterator<Requirement> for Requirements<T> {
    fn from_iter<V>(iter: V) -> Self
    where
        V: IntoIterator<Item = Requirement>,
    {
        Requirements {
            reqs: iter.into_iter().collect(),
            marker: std::marker::PhantomData::<T>,
        }
    }
}

impl Requirements<JobRequiredCaps> {
    pub(crate) fn requires_gpu(&self) -> bool {
        // TODO: can probably make Requirement<T> and work with
        // generics to remove this heap allocation
        self.reqs.contains(&Requirement("gpu".to_string()))
    }
}

impl<T> fmt::Display for Requirements<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let formatted = self
            .reqs
            .iter()
            .map(|x| format!("{}, ", x))
            .collect::<String>();
        write!(f, "{}", formatted)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodeProvidedCaps;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JobRequiredCaps;

#[derive(From, Ord, Eq, PartialEq, PartialOrd, Debug, Clone, Deserialize, Serialize, Display)]
pub struct Requirement(String);

impl From<&str> for Requirement {
    fn from(x: &str) -> Self {
        Requirement(x.to_string())
    }
}
