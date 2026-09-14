# Issue491: additional measurement axes

Parent-owned files: tools/performance_shapes.py, tests/test_performance_shapes.py, docs/performance/shapes.json. Coordinate new Rust gate entries with the Lean/adapter owner. Preserve21 existing active gates.

Design: extend the existing bounded fixture/runner instead of introducing a second runner. Each fixture declares expected candidate count, retained-plan count/truncation and CLI selectors/jobs/candidate cap independently. Verify partial plans accept exit4 and incomplete summary, while complete fixtures still require completeness. Save input files/bytes, output document bytes, process RSS and elapsed measurements separately; these are observations, not deterministic algorithmic costs or allocator peaks.

Add isolated N/2N/4N controls for selected-file count; repeated file/symbol/line declarations over fixed sources; fingerprint file count/bytes/exact+glob; paired Unicode long/short lines; wide AST/left depth/large unselected literal; active annotation collection; multifile and partial verify; output record length; workspace bytes/workers. Preserve existing zero-candidate and max-candidates1 controls. Bound workers to4 and ordinary execution cases to128 mutants, AST depth to128. Do not raise disk8GiB or lower free reserve10GiB.

Design reviews:1 compared every missing issue axis to current eleven fixtures;2 kept source shape independent from selector repetition and identified where candidate cap intentionally bounds discovery;3 separated sampled whole-process RSS from preflight allocator peak (Rust adapter owner handles separate peak observation).

Plan:1 write fixture semantic regressions and observe missing-shape RED;2 extend fixture declarations and runner validation (partial-plan completeness is explicit, not globally relaxed);3 add bounded registry rows;4 normal Python tests, exact selected mutation, real release smoke then three-repetition N/2N/4N baseline/candidate measurements;5 update audit evidence and source hashes with owner.

Plan reviews:1 ensure minsize1 remains compilable and partial fixture hasN+1available/Nretained even at1;2 expected plan size differs from top1 selected size;3 existing test gate/nonobservedRSS/failure artifact contracts remain enforced. New assertion covers actual files/bytes and public report record sizes, not only registry existence.
