# Exact-head permanent gate trigger

This commit exists to ensure the permanent Quality Gate runs on a normal source
head after all fail-closed Step 4 finalization checks. It carries no acceptance
claim by itself; acceptance depends on the exact-head four-job result and the
absence of temporary workflows or diagnostics.
