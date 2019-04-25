#![feature(never_type)]
#![feature(async_await)]
#![feature(await_macro)]
#![feature(gen_future)]
#![feature(rustc_attrs)]
#![feature(test)]

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

#[derive(Clone, Deserialize)]
struct SequenceOpts {
    base: Option<String>,

    #[serde(rename = "expandContext")]
    expand_context: Option<String>,

    #[serde(rename = "processingMode")]
    processing_mode: Option<String>,
}

#[derive(Clone, Deserialize)]
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

#[derive(Clone, Deserialize)]
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

fn wait<I, E>(f: impl Future<Output = Result<I, E>>) -> Result<I, E> {
    use tokio::prelude::Future;
    use tokio_async_await::compat::backward;
    backward::Compat::new(f).wait()
}

fn run_single_seq(seq: FakeSequence, iri: &str) -> Result<(), String> {
    if let Some(processing_mode) = seq.option.as_ref().and_then(|f| f.processing_mode.as_ref()) {
        if processing_mode == "json-ld-1.1" {
            return Ok(());
        }
    }

    if !seq.types.iter().any(|f| f == "jld:PositiveEvaluationTest") {
        return Ok(());
    }

    let input = get_data(&seq.input);
    let expect = get_data(&seq.expect);

    // println!("{} {}\n: {:?}", seq.id, seq.name, seq.purpose);

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

    let res = match wait(future) {
        Ok(res) => res,
        Err(err) => return Err(err.to_string()),
    };

    if expect != res {
        // println!(
        //     "Diff: {}\n{}\n------",
        //     serde_json::to_string_pretty(&expect).unwrap(),
        //     serde_json::to_string_pretty(&res).unwrap()
        // );
        Err("Mismatch!".to_string())
    } else {
        // println!("Ok!\n------");
        Ok(())
    }
}

#[macro_use]
extern crate lazy_static;
extern crate test as test;

lazy_static! {
    static ref DATA: FakeManifest =
        serde_json::from_value(get_data("expand-manifest.jsonld")).unwrap();
}

fn the_tests() -> Vec<test::TestDescAndFn> {
    DATA.sequence
        .iter()
        .cloned()
        .map(|seq| {
            let name = format!("{} ({})", seq.input, seq.name);

            test::TestDescAndFn {
                desc: test::TestDesc {
                    name: test::DynTestName(name),
                    ignore: false,
                    allow_fail: false,
                    should_panic: test::ShouldPanic::No,
                },
                testfn: test::DynTestFn(Box::new(|| {
                    test::assert_test_result(run_single_seq(seq, &DATA.base_iri))
                })),
            }
        })
        .collect()
}

fn main() -> () {
    extern crate test as test;
    let args = std::env::args().collect::<Vec<_>>();
    let opts = test::Options::new();
    test::test_main(&args, the_tests(), opts)
}
