# Cleanup Reservation Acknowledgement Implementation Plan

1. Add failing state-machine tests for missing, partial, duplicate, extra, and reordered IDs.
2. Retain expected cleanup reservation IDs in pending-effect metadata.
3. Validate exact set equality before retiring the effect.
4. Verify invalid acknowledgements preserve the grant and cannot finish the run.
5. Run core tests, contracts, formatting, linting, and repository tests.

