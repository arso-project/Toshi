use std::fmt;
use std::marker::PhantomData;

use serde::de::{Deserializer, Error as SerdeError, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::Serializer;
use serde::{Deserialize, Serialize};
use tantivy::query::Query as TantivyQuery;
use tantivy::schema::Schema;
use tantivy::Term;

pub use {
    self::bool::BoolQuery,
    self::facet::FacetQuery,
    self::fuzzy::{FuzzyQuery, FuzzyTerm},
    self::phrase::{PhraseQuery, TermPair},
    self::range::{RangeQuery, Ranges},
    self::regex::RegexQuery,
    self::term::ExactTerm,
};

use crate::{error::Error, Result};

struct Settings {}
impl Settings {
    pub fn default_result_limit() -> usize {
        100
    }
}

mod agg;
mod bool;
mod facet;
mod fuzzy;
mod phrase;
mod range;
mod regex;
mod term;

pub trait CreateQuery {
    fn create_query(self, schema: &Schema) -> Result<Box<dyn TantivyQuery>>;
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(untagged)]
pub enum Query {
    Boolean { bool: BoolQuery },
    Fuzzy(FuzzyQuery),
    Exact(ExactTerm),
    Phrase(PhraseQuery),
    Regex(RegexQuery),
    Range(RangeQuery),
    Raw { raw: String },
    All,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(untagged)]
pub enum Aggregates {
    SumAgg { field: String },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Search {
    #[serde(default = "Search::all")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<Query>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facets: Option<FacetQuery>,
    #[serde(default = "Settings::default_result_limit")]
    pub limit: usize,
}

impl Search {
    pub fn new(query: Option<Query>, facets: Option<FacetQuery>, limit: usize) -> Self {
        Search { query, facets, limit }
    }

    pub fn all() -> Option<Query> {
        Some(Query::All)
    }

    pub fn all_docs() -> Self {
        Self {
            query: Some(Query::All),
            facets: None,
            limit: Settings::default_result_limit(),
        }
    }
}

fn make_field_value(schema: &Schema, k: &str, v: &str) -> Result<Term> {
    let field = schema
        .get_field(k)
        .ok_or_else(|| Error::QueryError(format!("Field: {} does not exist", k)))?;
    Ok(Term::from_field_text(field, v))
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyValue<T> {
    pub field: String,
    pub value: T,
}

impl<T> KeyValue<T> {
    pub fn new(field: String, value: T) -> Self {
        KeyValue { field, value }
    }
}

struct KVVisitor<T> {
    marker: PhantomData<fn() -> KeyValue<T>>,
}

impl<T> KVVisitor<T> {
    pub fn new() -> Self {
        KVVisitor { marker: PhantomData }
    }
}

impl<'de, T> Visitor<'de> for KVVisitor<T>
where
    T: Deserialize<'de>,
{
    type Value = KeyValue<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an object with a single string value of any key name")
    }

    fn visit_map<M>(self, mut access: M) -> std::result::Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        if let Some((field, value)) = access.next_entry()? {
            if access.next_entry::<String, T>()?.is_some() {
                Err(M::Error::custom("too many values"))
            } else {
                Ok(KeyValue { field, value })
            }
        } else {
            Err(M::Error::custom("not enough values"))
        }
    }
}

impl<'de, T> Deserialize<'de> for KeyValue<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(KVVisitor::new())
    }
}

impl<T> Serialize for KeyValue<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut m = serializer.serialize_map(Some(1))?;
        m.serialize_entry(&self.field, &self.value)?;
        m.end()
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_kv_serialize() {
        let kv = KeyValue::new("test_field".into(), 1);
        let expected = r#"{"test_field":1}"#;
        assert_eq!(expected, serde_json::to_string(&kv).unwrap());
    }
}
