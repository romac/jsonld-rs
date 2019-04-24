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

use jsonld::{expand, JsonLdOptions, RemoteContextLoader};
use serde_json::Value;

use std::fs::File;
use std::future::*;
use std::pin::Pin;

#[derive(Deserialize)]
struct SequenceOpts {
    base: Option<String>,

    #[serde(rename = "expandContext")]
    expand_context: Option<String>,

    #[serde(rename = "processingMode")]
    processing_mode: Option<String>,
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
    expect: String,
    option: Option<SequenceOpts>,
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

fn run_single_seq(seq: FakeSequence, iri: &str) {
    if let Some(processing_mode) = seq.option.as_ref().and_then(|f| f.processing_mode.as_ref()) {
        if processing_mode == "json-ld-1.1" {
            return;
        }
    }

    if !seq.types.iter().any(|f| f == "jld:PositiveEvaluationTest") {
        return;
    }

    let input = get_data(&seq.input);
    let expect = get_data(&seq.expect);

    println!("{} {}\n: {:?}", seq.id, seq.name, seq.purpose);

    let base_iri = seq
        .option
        .as_ref()
        .and_then(|f| f.base.to_owned())
        .or_else(|| Some(iri.to_owned() + &seq.input));

    let ctx = seq
        .option
        .as_ref()
        .and_then(|f| f.expand_context.as_ref())
        .and_then(|f| Some(get_data(f)));

    let future = expand::<TestContextLoader>(
        input,
        JsonLdOptions {
            base: base_iri,
            compact_arrays: None,
            expand_context: ctx,
            processing_mode: None,
        },
    );

    let res = wait(future);

    let res = if let Ok(res) = res { res } else { return };

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

#[test]
fn test() {
    let data: FakeManifest = serde_json::from_value(get_data("expand-manifest.jsonld")).unwrap();
    for seq in data.sequence {
        run_single_seq(seq, &data.base_iri);
    }
}
