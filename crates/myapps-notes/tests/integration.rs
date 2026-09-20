// Every crate's integration test groups its areas under a module named after
// the app (`mod leanfin { pub mod accounts; … }`). Here one of the areas is
// also called `notes`, which trips module_inception; keeping the convention is
// worth more than the rename.
#[allow(clippy::module_inception)]
mod notes {
    pub mod notes;
    pub mod sync;
}
