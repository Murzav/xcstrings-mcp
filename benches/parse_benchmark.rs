use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

fn generate_fixture(keys: usize) -> String {
    let mut strings = String::new();
    for i in 0..keys {
        if i > 0 {
            strings.push(',');
        }
        strings.push_str(&format!(
            r#"
    "key_{i}" : {{
      "localizations" : {{
        "en" : {{
          "stringUnit" : {{
            "state" : "translated",
            "value" : "Value {i}"
          }}
        }},
        "de" : {{
          "stringUnit" : {{
            "state" : "translated",
            "value" : "Wert {i}"
          }}
        }}
      }}
    }}"#
        ));
    }
    format!(
        r#"{{
  "sourceLanguage" : "en",
  "strings" : {{{strings}
  }},
  "version" : "1.0"
}}"#
    )
}

fn parse_benchmark(c: &mut Criterion) {
    let fixture = generate_fixture(1000);

    c.bench_function("parse_1k_keys", |b| {
        b.iter(|| {
            let file: xcstrings_mcp::XcStringsFile =
                serde_json::from_str(black_box(&fixture)).unwrap();
            black_box(file);
        });
    });
}

criterion_group!(benches, parse_benchmark);
criterion_main!(benches);
