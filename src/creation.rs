use super::context::{Context, Term};
use super::RemoteContextLoader;

use serde_json::Map as JsonMap;
use serde_json::Value;

use std::clone::Clone;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

use url::Url;

pub enum DefineStatus {
    Defining,
    Defined,
}

#[derive(Debug)]
pub enum TermCreationError {
    CyclicIRIMapping,      //"cyclic IRI mapping"
    KeywordRedefinition,   // "keyword redefinition";
    InvalidTermDefinition, // "invalid term definition"
    InvalidIRIMapping,     // "invalid IRI mapping"
    InvalidReverseProperty,
    InvalidKeywordAlias,
    InvalidContainerMapping,
    InvalidLanguageMapping,
    InvalidTypeMapping,
}

impl fmt::Display for TermCreationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.description())
    }
}

impl Error for TermCreationError {
    fn description(&self) -> &str {
        match *self {
            TermCreationError::CyclicIRIMapping => "cyclic IRI mapping",
            TermCreationError::KeywordRedefinition => "keyword redefinition",
            TermCreationError::InvalidTermDefinition => "invalid term definition",
            TermCreationError::InvalidIRIMapping => "invalid IRI mapping",
            TermCreationError::InvalidReverseProperty => "invalid reverse property",
            TermCreationError::InvalidKeywordAlias => "invalid keyword alias",
            TermCreationError::InvalidContainerMapping => "invalid container mapping",
            TermCreationError::InvalidLanguageMapping => "invalid language mapping",
            TermCreationError::InvalidTypeMapping => "invalid type mapping",
        }
    }

    fn cause(&self) -> Option<&dyn Error> {
        None
    }
}

#[derive(Debug)]
pub enum ContextCreationError<T: RemoteContextLoader> {
    InvalidTerm(TermCreationError),
    RemoteContextError(T::Error),
    RemoteContextNoObject,

    RecursiveContextInclusion,
    InvalidBaseIRI,
    InvalidVocabMapping,
    InvalidLanguageMapping,
    InvalidLocalContext,

    TooManyContexts,
}

impl<T: RemoteContextLoader> fmt::Display for ContextCreationError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ContextCreationError::InvalidTerm(ref err) => write!(f, "invalid term: {}", err),
            ContextCreationError::RemoteContextError(ref err) => {
                write!(f, "invalid remote context: {}", err)
            }
            _ => f.write_str(self.description()),
        }
    }
}

impl<T: RemoteContextLoader> Error for ContextCreationError<T> {
    fn description(&self) -> &str {
        match *self {
            ContextCreationError::InvalidTerm(_) => "invalid term",
            ContextCreationError::RemoteContextError(_) => "invalid remote context",
            ContextCreationError::RemoteContextNoObject => "invalid remote context",
            ContextCreationError::RecursiveContextInclusion => "recursive context inclusion",
            ContextCreationError::InvalidBaseIRI => "invalid base IRI",
            ContextCreationError::InvalidVocabMapping => "invalid vocab mapping",
            ContextCreationError::InvalidLanguageMapping => "invalid language mapping",
            ContextCreationError::InvalidLocalContext => "invalid local context",
            ContextCreationError::TooManyContexts => "too many contexts",
        }
    }

    fn cause(&self) -> Option<&dyn Error> {
        match *self {
            ContextCreationError::InvalidTerm(ref err) => Some(err),
            ContextCreationError::RemoteContextError(_) => None,
            _ => None,
        }
    }
}

lazy_static! {
    static ref KEYWORDS: HashSet<&'static str> = vec![
        "@context",
        "@id",
        "@value",
        "@language",
        "@type",
        "@container",
        "@list",
        "@set",
        "@reverse",
        "@index",
        "@base",
        "@vocab",
        "@graph",
    ]
    .into_iter()
    .collect();
}

impl Context {
    pub fn new() -> Context {
        Context {
            base_iri: None,
            vocabulary_mapping: None,
            language: None,
            terms: BTreeMap::new(),
        }
    }

    pub(crate) fn expand_iri_mut(
        &mut self,
        val: &str,
        document_relative: bool,
        vocab: bool,
        defined: &mut HashMap<String, DefineStatus>,
        context: &mut JsonMap<String, Value>,
    ) -> Result<String, TermCreationError> {
        if val.starts_with("@") {
            // 1
            Ok(val.to_string())
        } else {
            // 2
            if context.contains_key(val) && !defined.contains_key(val) {
                let unwrapped = context.remove(val).unwrap();
                self.create_term(context, val, unwrapped, defined)?;
            }

            if vocab && self.terms.contains_key(val) {
                // 3
                let term = self.terms.get(val).unwrap();

                Ok(term.iri_mapping.clone())
            } else {
                // 4
                if let Some(loc) = val.find(":") {
                    // 4.1
                    let prefix = &val[..loc];
                    let suffix = &val[loc + 1..];

                    if prefix == "_" || suffix.starts_with("//") {
                        // 4.2
                        Ok(val.to_owned())
                    } else {
                        // 4.3
                        if context.contains_key(prefix) && !defined.contains_key(prefix) {
                            let unwrapped = context.remove(prefix).unwrap();
                            self.create_term(context, prefix, unwrapped, defined)?;
                        }

                        if let Some(ref term) = self.terms.get(prefix).as_ref() {
                            // 4.4
                            Ok(term.iri_mapping.clone() + suffix)
                        } else {
                            // 4.5
                            Ok(val.to_owned())
                        }
                    }
                } else {
                    if vocab && self.vocabulary_mapping.is_some() {
                        // 5
                        Ok(self.vocabulary_mapping.as_ref().unwrap().to_string() + val)
                    } else if document_relative && self.base_iri.is_some() {
                        // 6
                        let base_iri = self.base_iri.as_ref().unwrap();
                        let joined = base_iri.join(val).unwrap();
                        Ok(joined.to_string())
                    } else {
                        // 7
                        Ok(val.to_string())
                    }
                }
            }
        }
    }

    pub(crate) fn expand_iri(&self, val: &str, document_relative: bool, vocab: bool) -> String {
        if val.starts_with("@") {
            // 1
            val.to_string()
        } else {
            if vocab && self.terms.contains_key(val) {
                // 3
                let term = self.terms.get(val).unwrap();

                term.iri_mapping.clone()
            } else {
                // 4
                if let Some(loc) = val.find(":") {
                    // 4.1
                    let prefix = &val[..loc];
                    let suffix = &val[loc + 1..];

                    if prefix == "_" || suffix.starts_with("//") {
                        // 4.2
                        val.to_owned()
                    } else {
                        if let Some(term) = self.terms.get(prefix).as_ref() {
                            // 4.4
                            term.iri_mapping.clone() + suffix
                        } else {
                            // 4.5
                            val.to_owned()
                        }
                    }
                } else {
                    if vocab && self.vocabulary_mapping.is_some() {
                        // 5
                        self.vocabulary_mapping.as_ref().unwrap().to_string() + val
                    } else if document_relative && self.base_iri.is_some() {
                        // 6
                        let base_iri = self.base_iri.as_ref().unwrap();
                        let joined = base_iri.join(val).unwrap();
                        joined.to_string()
                    } else {
                        // 7
                        val.to_string()
                    }
                }
            }
        }
    }

    fn create_term<'a>(
        &mut self,
        context: &mut JsonMap<String, Value>,
        term: &'a str,
        mut value: Value,
        defined: &mut HashMap<String, DefineStatus>,
    ) -> Result<(), TermCreationError> {
        // 1
        if let Some(stat) = defined.get(term) {
            return match *stat {
                DefineStatus::Defining => Err(TermCreationError::CyclicIRIMapping),
                DefineStatus::Defined => Ok(()),
            };
        }

        // 2
        defined.insert(term.to_owned(), DefineStatus::Defining);

        // 3
        if KEYWORDS.contains(term) {
            return Err(TermCreationError::KeywordRedefinition);
        }

        // 4
        self.terms.remove(term);

        // 5: implicit???

        // 7
        if let Value::String(string) = value {
            let mut map = JsonMap::new();
            map.insert("@id".to_owned(), Value::String(string));
            value = Value::Object(map);
        };

        match value {
            // 6, todo @id: null
            Value::Null => {
                // XXX really bad hack to avoid @vocab???
                self.terms.insert(
                    term.to_owned(),
                    Term {
                        type_mapping: None,
                        iri_mapping: term.to_owned(),
                        reverse: false,
                        container_mapping: None,
                        language_mapping: None,
                    },
                );
            }
            Value::String(_) => unreachable!(),
            Value::Object(mut map) => {
                // 10, 10.3
                let type_mapping = if let Some(at_type) = map.remove("@type") {
                    match at_type {
                        Value::String(string) => {
                            // 10.2
                            let res =
                                self.expand_iri_mut(&string, false, true, defined, context)?;
                            if !res.contains(":") && res != "@id" && res != "@vocab" {
                                return Err(TermCreationError::InvalidTypeMapping);
                            }
                            Some(res)
                        }
                        // 10.1
                        _ => return Err(TermCreationError::InvalidTypeMapping),
                    }
                } else {
                    None
                };

                // 11
                if let Some(at_reverse) = map.remove("@reverse") {
                    // 11.1
                    if map.contains_key("@id") {
                        return Err(TermCreationError::InvalidReverseProperty);
                    }

                    let reverse_map = match at_reverse {
                        Value::String(string) => {
                            // 10.2
                            let res =
                                self.expand_iri_mut(&string, false, true, defined, context)?;
                            if !res.contains(":") {
                                return Err(TermCreationError::InvalidIRIMapping);
                            }
                            Some(res)
                        }

                        // 11.2
                        _ => return Err(TermCreationError::InvalidIRIMapping),
                    };

                    // 11.4
                    let container_mapping = if let Some(at_container) = map.remove("@container") {
                        match at_container {
                            Value::String(string) => {
                                if string == "@set" || string == "@index" {
                                    Some(string)
                                } else {
                                    return Err(TermCreationError::InvalidReverseProperty);
                                }
                            }
                            Value::Null => None,
                            _ => return Err(TermCreationError::InvalidReverseProperty),
                        }
                    } else {
                        None
                    };

                    // 11.6
                    defined.insert(term.to_owned(), DefineStatus::Defined);

                    self.terms.insert(
                        term.to_owned(),
                        Term {
                            type_mapping: type_mapping,
                            iri_mapping: reverse_map.unwrap(),
                            reverse: true,
                            container_mapping: container_mapping,
                            language_mapping: None,
                        },
                    );
                } else {
                    // 13
                    let mut iri_mapping = if let Some(at_id) = map.remove("@id") {
                        match at_id {
                            Value::String(string) => {
                                if string == term {
                                    None
                                } else {
                                    // 13.2
                                    let expanded = self
                                        .expand_iri_mut(&string, false, true, defined, context)?;
                                    if expanded == "@context" {
                                        return Err(TermCreationError::InvalidKeywordAlias);
                                    } else if !expanded.starts_with("@")
                                        && !expanded.starts_with("_:")
                                        && !expanded.contains("://")
                                    {
                                        return Err(TermCreationError::InvalidIRIMapping);
                                    } else {
                                        Some(expanded)
                                    }
                                }
                            }

                            Value::Null => Some(term.to_owned()),
                            // 13.1
                            _ => return Err(TermCreationError::InvalidIRIMapping),
                        }
                    } else {
                        None
                    };

                    // 14
                    if iri_mapping == None && term.contains(":") {
                        let (first, second) = term.split_at(term.find(":").unwrap());

                        // 14.1
                        if context.contains_key(first) {
                            let term_value = context.remove(first).unwrap();
                            self.create_term(context, first, term_value, defined)?;
                        }

                        // 14.2
                        if self.terms.contains_key(first) {
                            let mut new_iri_mapping = "".to_owned();
                            let first_term = self.terms.get(first).unwrap();
                            new_iri_mapping.push_str(&first_term.iri_mapping);
                            new_iri_mapping.push_str(&second[1..]);

                            iri_mapping = Some(new_iri_mapping);
                        } else {
                            // 14.3
                            iri_mapping = Some(term.to_string());
                        }
                    }

                    // 15
                    if iri_mapping == None {
                        if let Some(ref vocab) = self.vocabulary_mapping {
                            let mut new_iri_mapping = vocab.to_string();
                            new_iri_mapping.push_str(term);
                            iri_mapping = Some(new_iri_mapping);
                        } else {
                            return Err(TermCreationError::InvalidIRIMapping);
                        }
                    }

                    // 16, 16.2
                    let container_mapping = if let Some(at_container) = map.remove("@container") {
                        match at_container {
                            Value::String(string) => {
                                // 16.1
                                if string == "@list"
                                    || string == "@set"
                                    || string == "@index"
                                    || string == "@language"
                                {
                                    Some(string)
                                } else {
                                    return Err(TermCreationError::InvalidContainerMapping);
                                }
                            }
                            _ => return Err(TermCreationError::InvalidContainerMapping),
                        }
                    } else {
                        None
                    };

                    // 17
                    let language_mapping = if type_mapping != None {
                        None
                    } else {
                        if let Some(language) = map.remove("@language") {
                            match language {
                                // 17.2
                                Value::String(string) => Some(string.to_lowercase()),
                                Value::Null => Some("@null".to_owned()),

                                // 17.1
                                _ => return Err(TermCreationError::InvalidLanguageMapping),
                            }
                        } else {
                            None
                        }
                    };

                    // 18
                    defined.insert(term.to_string(), DefineStatus::Defined);
                    self.terms.insert(
                        term.to_string(),
                        Term {
                            type_mapping: type_mapping,
                            iri_mapping: iri_mapping.unwrap(),
                            reverse: false,
                            container_mapping: container_mapping,
                            language_mapping: language_mapping,
                        },
                    );
                }
            }
            // 8
            _ => return Err(TermCreationError::InvalidTermDefinition),
        };

        Ok(())
    }

    pub async fn process_context<T: RemoteContextLoader>(
        mut self,
        local_context: Value,
        mut remote_contexts: HashMap<String, Option<Value>>,
    ) -> Result<(HashMap<String, Option<Value>>, Context), ContextCreationError<T>> {
        // 2
        let local_context = match local_context {
            Value::Array(a) => a,
            _ => vec![local_context],
        };

        // 3
        for context in local_context {
            match context {
                // 3.1
                Value::Null => {
                    self = Context::new();
                    self.base_iri = self.base_iri.clone();
                }

                // 3.2
                Value::String(val) => {
                    if remote_contexts.len() > 4 {
                        return Err(ContextCreationError::TooManyContexts);
                    }

                    match remote_contexts.get(&val).cloned() {
                        Some(None) => return Err(ContextCreationError::RecursiveContextInclusion),
                        Some(Some(context)) => {
                            let pinned: Pin<Box<dyn Future<Output = _>>> = Box::pin(
                                self.process_context::<T>(context.clone(), remote_contexts),
                            );
                            let (rc, s) = await!(pinned)?;
                            remote_contexts = rc;
                            remote_contexts.insert(val, Some(context));

                            self = s;
                        }

                        None => {
                            // 3.2.3
                            let dereferenced = await!(T::load_context(val.to_owned()))
                                .map_err(|e| ContextCreationError::RemoteContextError(e))?;
                            remote_contexts.insert(val.to_owned(), None);

                            if let Value::Object(mut obj) = dereferenced {
                                let context = obj
                                    .remove("@context")
                                    .unwrap_or_else(|| Value::Object(JsonMap::new()));

                                // 3.2.4
                                let pinned: Pin<Box<dyn Future<Output = _>>> = Box::pin(
                                    self.process_context::<T>(context.clone(), remote_contexts),
                                );
                                let (rc, s) = await!(pinned)?;
                                remote_contexts = rc;
                                remote_contexts.insert(val, Some(context));

                                self = s;
                            } else {
                                return Err(ContextCreationError::RemoteContextNoObject);
                            }
                        }
                    }
                }
                Value::Object(mut map) => {
                    let base = map.remove("@base");
                    if base != None && remote_contexts.is_empty() {
                        let value = base.unwrap();
                        match value {
                            Value::Null => self.base_iri = None,
                            Value::String(val) => {
                                if let Some(iri) = self.base_iri {
                                    self.base_iri = Some(
                                        iri.join(&val)
                                            .map_err(|_| ContextCreationError::InvalidBaseIRI)?,
                                    );
                                } else {
                                    self.base_iri = Some(
                                        Url::parse(&val)
                                            .map_err(|_| ContextCreationError::InvalidBaseIRI)?,
                                    );
                                }
                            }
                            _ => return Err(ContextCreationError::InvalidBaseIRI),
                        }
                    };

                    // 3.5
                    if let Some(vocab) = map.remove("@vocab") {
                        match vocab {
                            Value::Null => self.vocabulary_mapping = None,
                            Value::String(data) => {
                                // xxx proper absolute iri processor
                                self.vocabulary_mapping = Some(data);
                            }
                            _ => return Err(ContextCreationError::InvalidVocabMapping),
                        }
                    }

                    if let Some(language) = map.remove("@language") {
                        match language {
                            Value::Null => self.language = None,
                            Value::String(data) => {
                                self.language = Some(data.to_lowercase());
                            }

                            _ => return Err(ContextCreationError::InvalidLanguageMapping),
                        }
                    }

                    let mut defined: HashMap<String, DefineStatus> = HashMap::new();

                    while !map.is_empty() {
                        let key = map.keys().next().unwrap().clone();
                        let val = map.remove(&key).unwrap();
                        self.create_term(&mut map, &key, val, &mut defined)
                            .map_err(|e| ContextCreationError::InvalidTerm(e))?;
                    }
                }
                // 3.3
                _ => return Err(ContextCreationError::InvalidLocalContext),
            }
        }

        Ok((remote_contexts, self))
    }
}
