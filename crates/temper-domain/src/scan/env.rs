//! The per-scan view of one source that the rules read (1.x `makeEnv`).

use std::cell::OnceCell;
use std::collections::BTreeSet;

use super::doc_blocks::{DocBlockPatterns, DocBlocks, doc_blocks};
use super::lexer::{Comment, LineIndex, Masked};
use super::regions::{RegionPatterns, Regions, const_regions, test_regions, trait_impl_regions};
use super::scanner::{CodeKind, FileRole, ScanOptions};

/// One source as the rules see it. Borrows the source and its masking; owns the line index and
/// the test regions it derives from them.
#[derive(Debug)]
pub(crate) struct Env<'s> {
    /// The source as written.
    pub(crate) src: &'s str,
    /// The masked source.
    pub(crate) code: &'s str,
    /// The source's comments, in order.
    pub(crate) comments: &'s [Comment<'s>],
    /// Lines of the masked source (the same as the source's).
    pub(crate) idx: LineIndex,
    pub(crate) regions: Regions,
    pub(crate) kind: CodeKind,
    pub(crate) file: FileRole,
    /// The lints the crate forbids, which `forbid-override` reads.
    pub(crate) forbid: Option<&'s BTreeSet<String>>,
    // Interior mutability (OWN-12): each is computed on its first read through `&self` and never
    // changes after, as 1.x's lazy `consts`, `traitImpls` and `doc` getters; most scans read
    // none of them.
    consts: OnceCell<Regions>,
    trait_impls: OnceCell<Regions>,
    doc: OnceCell<DocBlocks<'s>>,
}

impl<'s> Env<'s> {
    /// Indexes the lines of `src`'s `masked` code and finds its test regions.
    pub(crate) fn new(
        src: &'s str,
        masked: &'s Masked<'s>,
        options: &ScanOptions<'s>,
        patterns: &RegionPatterns,
    ) -> Self {
        let code = masked.code.as_str();
        Self {
            src,
            code,
            comments: &masked.comments,
            idx: LineIndex::new(code),
            regions: test_regions(code, patterns),
            kind: options.kind,
            file: options.file,
            forbid: options.forbid,
            consts: OnceCell::new(),
            trait_impls: OnceCell::new(),
            doc: OnceCell::new(),
        }
    }

    /// The trait impl bodies of the code, computed on first use.
    pub(crate) fn trait_impls(&self, patterns: &RegionPatterns) -> &Regions {
        self.trait_impls
            .get_or_init(|| trait_impl_regions(self.code, patterns))
    }

    /// The compile-time regions of the code, computed on first use.
    pub(crate) fn consts(&self, patterns: &RegionPatterns) -> &Regions {
        self.consts
            .get_or_init(|| const_regions(self.code, patterns))
    }

    /// The doc comments of the source, split into fences and prose, computed on first use.
    pub(crate) fn doc(&self, patterns: &DocBlockPatterns) -> &DocBlocks<'s> {
        self.doc
            .get_or_init(|| doc_blocks(self.code, self.comments, patterns))
    }

    /// The context of the code at `offset`: test inside a test region, else the file's.
    pub(crate) fn context_at(&self, offset: usize) -> CodeKind {
        if self.kind == CodeKind::Test || self.regions.contains(offset) {
            CodeKind::Test
        } else {
            self.kind
        }
    }
}
