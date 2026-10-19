---
type: regex
target: { source: file, path: crates/ingest/src/lib.rs }
# An audit reports and changes no code: the planted lines are still there.
pattern: 'let kind = headers\["kind"\]\.to_string\(\);[\s\S]*let average = total / count;[\s\S]*let retry_after = Duration::from_secs_f64\(retry\);[\s\S]*let len = body\.len\(\) as u16;'
---
