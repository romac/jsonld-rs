#![feature(never_type)]
#![feature(futures_api)]
#![feature(async_await)]
#![feature(await_macro)]
#![feature(gen_future)]

extern crate jsonld;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate url;
extern crate tokio;

use jsonld::{compact, JsonLdOptions, RemoteContextLoader};

use serde_json::Value;

use std::fs::File;
use std::future::*;
use std::pin::Pin;

#[derive(Deserialize)]
struct SequenceOptions {
    #[serde(rename = "compactArrays")]
    compact_arrays: Option<bool>,

    #[serde(rename = "specVersion")]
    spec_version: Option<String>,
}

#[derive(Deserialize)]
struct FakeSequence {
    #[serde(rename = "@id")]
    id: String,

    #[serde(rename = "@type")]
    types: Vec<String>,

    name: String,
    purpose: Option<String>,
    input: String,
    context: String,
    expect: String,
    option: Option<SequenceOptions>,
}

#[derive(Deserialize)]
struct FakeManifest {
    #[serde(rename = "baseIri")]
    base_iri: String,
    sequence: Vec<FakeSequence>,
}

#[derive(Debug)]
struct TestContextLoader {}

impl RemoteContextLoader for TestContextLoader {
    type Error = !;
    type Future = Pin<Box<Future<Output = Result<Value, Self::Error>>>>;

    fn load_context(_url: String) -> Self::Future {
        Box::pin(async { Ok(Value::Null) })
    }
}

fn get_data(name: &str) -> Value {
    let f = File::open(&("resources/".to_owned() + name)).expect("file fail");

    serde_json::from_reader(f).expect("json fail")
}

fn wait<I, E>(f: impl Future<Output=Result<I, E>>) -> Result<I, E> {
    use tokio::prelude::Future;
    use tokio_async_await::compat::backward;
    backward::Compat::new(f).wait()
}

fn run_single_seq(seq: FakeSequence, base_iri: &str) {
    if !seq.types.iter().any(|f| f == "jld:PositiveEvaluationTest") {
        return;
    }

    if let Some(spec_version) = seq.option.as_ref().and_then(|f| f.spec_version.as_ref()) {
        if spec_version == "json-ld-1.1" {
            return;
        }
    }

    let input = get_data(&seq.input);
    let context = get_data(&seq.context);
    let expect = get_data(&seq.expect);

    println!("{} {}\n: {:?}", seq.id, seq.name, seq.purpose);

    println!("{:?}", context);

    let future = compact::<TestContextLoader>(
        input,
        context,
        JsonLdOptions {
            base: Some(base_iri.to_owned()),
            compact_arrays: seq.option.and_then(|f| f.compact_arrays),
            expand_context: None,
            processing_mode: None,
        },
    );

    let res = wait(future);

    match res {
        Ok(res) => {
            if expect != res {
                println!(
                    "Diff: {}\n{}\n------",
                    serde_json::to_string_pretty(&expect).unwrap(),
                    serde_json::to_string_pretty(&res).unwrap()
                );
            } else {
                println!("Ok!\n------");
            }
        }

        Err(e) => {
            println!("Fail: {}", e);
        }
    }
}

#[test]
fn test() {
    let data: FakeManifest = serde_json::from_value(get_data("compact-manifest.jsonld")).unwrap();
    for seq in data.sequence {
        run_single_seq(seq, &data.base_iri);
    }
}
