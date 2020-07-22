// Copyright (c) Facebook, Inc. and its affiliates
// SPDX-License-Identifier: MIT OR Apache-2.0

use serde::{Deserialize, Serialize};
use serde_generate::{cpp, java, python3, rust, test_utils};
use serde_reflection::{Registry, Result, Samples, Tracer, TracerConfig};
use std::fs::File;
use std::io::Write;
use std::process::Command;
use tempfile::tempdir;

#[derive(Serialize, Deserialize)]
struct Test {
    a: Vec<u32>,
    b: (i64, u64),
    c: Choice,
}

#[derive(Serialize, Deserialize)]
enum Choice {
    A,
    B(u64),
    C { x: u8 },
}

fn get_local_registry() -> Result<Registry> {
    let mut tracer = Tracer::new(TracerConfig::default());
    let samples = Samples::new();
    tracer.trace_type::<Test>(&samples)?;
    tracer.trace_type::<Choice>(&samples)?;
    Ok(tracer.registry()?)
}

#[test]
fn test_python_bincode_runtime_on_simple_data() {
    let registry = get_local_registry().unwrap();
    let dir = tempdir().unwrap();
    let source_path = dir.path().join("test.py");
    let mut source = File::create(&source_path).unwrap();
    python3::output(&mut source, &registry).unwrap();

    let reference = bincode::serialize(&Test {
        a: vec![4, 6],
        b: (3, 5),
        c: Choice::C { x: 7 },
    })
    .unwrap();
    writeln!(
        source,
        r#"
import bincode

value = Test([4, 6], (3, 5), Choice__C(7))

s = bincode.serialize(value, Test)
assert s == bytes.fromhex("{}")

v, buffer = bincode.deserialize(s, Test)
assert len(buffer) == 0
assert v == value
assert v.c.x == 7

v.b = (3, 0)
t = bincode.serialize(v, Test)
assert len(t) == len(s)
assert t != s
"#,
        hex::encode(&reference),
    )
    .unwrap();

    let python_path = std::env::var("PYTHONPATH").unwrap_or_default() + ":runtime/python";
    let status = Command::new("python3")
        .arg(source_path)
        .env("PYTHONPATH", python_path)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn test_python_bincode_runtime_on_all_supported_types() {
    let registry = test_utils::get_registry().unwrap();
    let dir = tempdir().unwrap();
    let source_path = dir.path().join("test.py");
    let mut source = File::create(&source_path).unwrap();
    python3::output(&mut source, &registry).unwrap();

    let values = test_utils::get_sample_values();
    let hex_encodings: Vec<_> = values
        .iter()
        .map(|v| format!("'{}'", hex::encode(&bincode::serialize(&v).unwrap())))
        .collect();

    writeln!(
        source,
        r#"
import bincode

encodings = [bytes.fromhex(s) for s in [{}]]

for encoding in encodings:
    v, buffer = bincode.deserialize(encoding, SerdeData)
    assert len(buffer) == 0

    s = bincode.serialize(v, SerdeData)
    assert s == encoding
"#,
        hex_encodings.join(", ")
    )
    .unwrap();

    let python_path = format!(
        "{}:runtime/python",
        std::env::var("PYTHONPATH").unwrap_or_default()
    );
    let status = Command::new("python3")
        .arg(source_path)
        .env("PYTHONPATH", python_path)
        .status()
        .unwrap();
    assert!(status.success());
}

// Full test using cargo. This may take a while.
#[test]
fn test_rust_bincode_runtime() {
    let registry = test_utils::get_registry().unwrap();
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        r#"[package]
name = "testing2"
version = "0.1.0"
edition = "2018"

[dependencies]
hex = "0.4.2"
serde = { version = "1.0", features = ["derive"] }
serde_bytes = "0.11"
bincode = "1.2"

[workspace]
"#,
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    let source_path = dir.path().join("src/main.rs");
    let mut source = File::create(&source_path).unwrap();
    rust::output(&mut source, /* with_derive_macros */ true, &registry).unwrap();

    let values = test_utils::get_sample_values();
    let hex_encodings: Vec<_> = values
        .iter()
        .map(|v| format!("\"{}\"", hex::encode(&bincode::serialize(&v).unwrap())))
        .collect();

    writeln!(
        source,
        r#"
fn main() {{
    let hex_encodings = vec![{}];

    for hex_encoding in hex_encodings {{
        let encoding = hex::decode(hex_encoding).unwrap();
        let value = bincode::deserialize::<SerdeData>(&encoding).unwrap();

        let s = bincode::serialize(&value).unwrap();
        assert_eq!(s, encoding);
    }}
}}
"#,
        hex_encodings.join(", ")
    )
    .unwrap();

    // Use a stable `target` dir to avoid downloading and recompiling crates everytime.
    let target_dir = std::env::current_dir().unwrap().join("../target");
    let status = Command::new("cargo")
        .current_dir(dir.path())
        .arg("run")
        .arg("--target-dir")
        .arg(target_dir)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn test_rust_documentation_on_simple_data() {
    let registry = get_local_registry().unwrap();
    let definitions = rust::quote_container_definitions(&registry).unwrap();
    assert_eq!(definitions.len(), 2);
    assert!(definitions.get("Test").unwrap().starts_with("struct Test"));
    assert!(definitions
        .get("Choice")
        .unwrap()
        .starts_with("enum Choice"));
}

#[test]
fn test_cpp_bincode_runtime_on_simple_data() {
    let registry = get_local_registry().unwrap();
    let dir = tempdir().unwrap();
    let header_path = dir.path().join("test.hpp");
    let mut header = File::create(&header_path).unwrap();
    cpp::output(&mut header, &registry, Some("test")).unwrap();

    let reference = bincode::serialize(&Test {
        a: vec![4, 6],
        b: (-3, 5),
        c: Choice::C { x: 7 },
    })
    .unwrap();

    let source_path = dir.path().join("test.cpp");
    let mut source = File::create(&source_path).unwrap();
    writeln!(
        source,
        r#"
#include <cassert>
#include "bincode.hpp"
#include "test.hpp"

using namespace serde;
using test::Choice;
using test::Test;

int main() {{
    std::vector<uint8_t> input = {{{}}};

    auto deserializer = BincodeDeserializer(input);
    auto test = Deserializable<Test>::deserialize(deserializer);

    auto a = std::vector<uint32_t> {{4, 6}};
    auto b = std::tuple<int64_t, uint64_t> {{-3, 5}};
    auto c = Choice {{ Choice::C {{ 7 }} }};
    auto test2 = Test {{a, b, c}};

    assert(test == test2);

    auto serializer = BincodeSerializer();
    Serializable<Test>::serialize(test2, serializer);
    auto output = std::move(serializer).bytes();

    assert(input == output);

    return 0;
}}
"#,
        reference
            .iter()
            .map(|x| format!("0x{:02x}", x))
            .collect::<Vec<_>>()
            .join(", ")
    )
    .unwrap();

    let status = Command::new("clang++")
        .arg("--std=c++17")
        .arg("-o")
        .arg(dir.path().join("test"))
        .arg("-I")
        .arg("runtime/cpp")
        .arg(source_path)
        .status()
        .unwrap();
    assert!(status.success());

    let status = Command::new(dir.path().join("test")).status().unwrap();
    assert!(status.success());
}

#[test]
fn test_cpp_bincode_runtime_on_supported_types() {
    let registry = test_utils::get_registry().unwrap();
    let dir = tempdir().unwrap();
    let header_path = dir.path().join("test.hpp");
    let mut header = File::create(&header_path).unwrap();
    cpp::output(&mut header, &registry, None).unwrap();

    let values = test_utils::get_sample_values();
    let encodings = values
        .iter()
        .map(|v| {
            let bytes = bincode::serialize(&v).unwrap();
            format!(
                "std::vector<uint8_t>{{{}}}",
                bytes
                    .iter()
                    .map(|x| format!("0x{:02x}", x))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    let source_path = dir.path().join("test.cpp");
    let mut source = File::create(&source_path).unwrap();
    writeln!(
        source,
        r#"
#include <cassert>
#include "bincode.hpp"
#include "test.hpp"

using namespace serde;

int main() {{
    std::vector<std::vector<uint8_t>> inputs = {{{}}};

    for (auto input: inputs) {{
        auto deserializer = BincodeDeserializer(input);
        auto test = Deserializable<SerdeData>::deserialize(deserializer);

        auto serializer = BincodeSerializer();
        Serializable<SerdeData>::serialize(test, serializer);
        auto output = std::move(serializer).bytes();

        assert(input == output);
    }}

    return 0;
}}
"#,
        encodings
    )
    .unwrap();

    let status = Command::new("clang++")
        .arg("--std=c++17")
        .arg("-g")
        .arg("-o")
        .arg(dir.path().join("test"))
        .arg("-I")
        .arg("runtime/cpp")
        .arg(source_path)
        .status()
        .unwrap();
    assert!(status.success());

    let status = Command::new(dir.path().join("test")).status().unwrap();
    assert!(status.success());
}

#[test]
fn test_java_bincode_runtime_on_simple_data() {
    let registry = get_local_registry().unwrap();
    let dir = tempdir().unwrap();

    let mut source = File::create(&dir.path().join("Testing.java")).unwrap();
    java::output(&mut source, &registry, "Testing").unwrap();

    let reference = bincode::serialize(&Test {
        a: vec![4, 6],
        b: (-3, 5),
        c: Choice::C { x: 7 },
    })
    .unwrap();

    let mut source = File::create(&dir.path().join("Main.java")).unwrap();
    writeln!(
        source,
        r#"
import java.util.List;
import java.util.Arrays;
import com.facebook.serde.Deserializer;
import com.facebook.serde.Serializer;
import com.facebook.serde.Unsigned;
import com.facebook.serde.Tuple2;
import com.facebook.bincode.BincodeDeserializer;
import com.facebook.bincode.BincodeSerializer;

public class Main {{
    public static void main(String[] args) throws java.lang.Exception {{
        byte[] input = new byte[] {{{}}};

        Deserializer deserializer = new BincodeDeserializer(input);
        Testing.Test test = Testing.Test.deserialize(deserializer);

        List<@Unsigned Integer> a = Arrays.asList(4, 6);
        Tuple2<Long, @Unsigned Long> b = new Tuple2<>(new Long(-3), new Long(5));
        Testing.Choice c = new Testing.Choice.C(new Byte((byte) 7));
        Testing.Test test2 = new Testing.Test(a, b, c);

        assert test.equals(test2);

        Serializer serializer = new BincodeSerializer();
        test2.serialize(serializer);
        byte[] output = serializer.get_bytes();

        assert java.util.Arrays.equals(input, output);
    }}
}}
"#,
        reference
            .iter()
            .map(|x| format!("{}", *x as i8))
            .collect::<Vec<_>>()
            .join(", ")
    )
    .unwrap();

    let paths = std::iter::empty()
        .chain(std::fs::read_dir("runtime/java/com/facebook/serde").unwrap())
        .chain(std::fs::read_dir("runtime/java/com/facebook/bincode").unwrap())
        .map(|e| e.unwrap().path());
    let status = Command::new("javac")
        .arg("-Xlint")
        .arg("-d")
        .arg(dir.path())
        .args(paths)
        .status()
        .unwrap();
    assert!(status.success());

    let status = Command::new("javac")
        .arg("-Xlint")
        .arg("-cp")
        .arg(dir.path())
        .arg("-d")
        .arg(dir.path())
        .arg(dir.path().join("Testing.java"))
        .arg(dir.path().join("Main.java"))
        .status()
        .unwrap();
    assert!(status.success());

    let status = Command::new("java")
        .arg("-enableassertions")
        .arg("-cp")
        .arg(dir.path())
        .arg("Main")
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn test_java_bincode_runtime_on_supported_types() {
    let registry = test_utils::get_registry().unwrap();
    let dir = tempdir().unwrap();

    let mut source = File::create(&dir.path().join("Testing.java")).unwrap();
    java::output(&mut source, &registry, "Testing").unwrap();

    let values = test_utils::get_sample_values();
    let encodings = values
        .iter()
        .map(|v| {
            let bytes = bincode::serialize(&v).unwrap();
            format!(
                "\n{{{}}}",
                bytes
                    .iter()
                    .map(|x| format!("{}", *x as i8))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    let mut source = File::create(&dir.path().join("Main.java")).unwrap();
    writeln!(
        source,
        r#"
import java.util.List;
import java.util.Arrays;
import com.facebook.serde.Deserializer;
import com.facebook.serde.Serializer;
import com.facebook.serde.Unsigned;
import com.facebook.serde.Tuple2;
import com.facebook.bincode.BincodeDeserializer;
import com.facebook.bincode.BincodeSerializer;

public class Main {{
    public static void main(String[] args) throws java.lang.Exception {{
        byte[][] inputs = new byte[][] {{{}}};

        for (int i = 0; i < inputs.length; i++) {{
            Deserializer deserializer = new BincodeDeserializer(inputs[i]);
            Testing.SerdeData test = Testing.SerdeData.deserialize(deserializer);

            Serializer serializer = new BincodeSerializer();
            test.serialize(serializer);
            byte[] output = serializer.get_bytes();

            assert java.util.Arrays.equals(inputs[i], output);
        }}
    }}
}}
"#,
        encodings
    )
    .unwrap();

    let paths = std::iter::empty()
        .chain(std::fs::read_dir("runtime/java/com/facebook/serde").unwrap())
        .chain(std::fs::read_dir("runtime/java/com/facebook/bincode").unwrap())
        .map(|e| e.unwrap().path());
    let status = Command::new("javac")
        .arg("-Xlint")
        .arg("-d")
        .arg(dir.path())
        .args(paths)
        .status()
        .unwrap();
    assert!(status.success());

    let status = Command::new("javac")
        .arg("-Xlint")
        .arg("-cp")
        .arg(dir.path())
        .arg("-d")
        .arg(dir.path())
        .arg(dir.path().join("Testing.java"))
        .arg(dir.path().join("Main.java"))
        .status()
        .unwrap();
    assert!(status.success());

    let status = Command::new("java")
        .arg("-enableassertions")
        .arg("-cp")
        .arg(dir.path())
        .arg("Main")
        .status()
        .unwrap();
    assert!(status.success());
}