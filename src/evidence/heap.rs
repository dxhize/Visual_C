//! Indexed heap-event ↔ allocator-call candidate and ambiguity records.

use std::path::Path;

use crate::clang_ast::SourceMetadataRegistry;
use crate::models::HeapEvent;

pub(super) fn correlate(registry: &mut SourceMetadataRegistry, events: &[HeapEvent], cwd: &Path) {
    crate::clang_ast::correlate_heap_calls(registry, events, cwd);
}
