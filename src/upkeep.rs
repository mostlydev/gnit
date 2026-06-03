pub fn run_transparent_upkeep(verbose: bool) {
    // Non-destructive, local-only maintenance belongs here:
    // local exclude repair, generated helper refresh, roster cache refresh,
    // and cached release metadata checks. This must stay fast, staleness-gated,
    // and no-op quiet.
    if verbose {
        eprintln!("nit upkeep: no pending local maintenance");
    }
}
