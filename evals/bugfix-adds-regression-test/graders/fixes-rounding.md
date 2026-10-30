---
type: regex
target: { source: file, path: crates/paging/src/lib.rs }
# The fix: page_count rounds up (div_ceil, or an explicit remainder check).
pattern: 'div_ceil|%\s*self\.per_page|self\.per_page\s*-\s*1|checked_rem|\.rem\('
---
