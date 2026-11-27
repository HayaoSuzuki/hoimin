import HoiminOracle.SurvivorReviewCases
import HoiminOracle.ReviewSourceCases
import HoiminOracle.ReviewReportAdmissionCases
import HoiminOracle.ReviewStoreDestinationCases

open HoiminOracle.SurvivorReview

def main (args : List String) : IO UInt32 := do
  let args := if args.head? = some "--" then args.drop 1 else args
  match args with
  | ["--output", path] =>
    IO.FS.writeFile path renderCorpus
    return 0
  | ["--check", path] =>
    return if (← IO.FS.readFile path) = renderCorpus then 0 else 1
  | ["--output-review-sources", path] =>
    IO.FS.writeFile path HoiminOracle.ReviewSource.corpus
    return 0
  | ["--check-review-sources", path] =>
    if (← IO.FS.readFile path) == HoiminOracle.ReviewSource.corpus then return 0
    IO.eprintln s!"stale review source corpus: {path}"
    return 1
  | ["--output-review-report-admission", path] =>
    IO.FS.writeFile path HoiminOracle.ReviewReportAdmission.corpus
    return 0
  | ["--check-review-report-admission", path] =>
    if (← IO.FS.readFile path) = HoiminOracle.ReviewReportAdmission.corpus then return 0
    IO.eprintln s!"stale review report admission corpus: {path}"
    return 1
  | ["--output-review-store-destination", path] =>
    IO.FS.writeFile path HoiminOracle.ReviewStoreDestination.corpus
    return 0
  | ["--check-review-store-destination", path] =>
    if (← IO.FS.readFile path) = HoiminOracle.ReviewStoreDestination.corpus then return 0
    IO.eprintln s!"stale review store destination corpus: {path}"
    return 1
  | ["--sensitivity"] =>
    IO.println s!"identity_and_revision_guards_detected={sensitivityPasses}"
    if !sensitivityPasses then return 2
    let status ← HoiminOracle.ReviewSource.sensitivity
    if status != 0 then return status
    let status ← HoiminOracle.ReviewReportAdmission.sensitivity
    if status != 0 then return status
    HoiminOracle.ReviewStoreDestination.sensitivity
  | _ =>
    IO.eprintln "usage: generate_survivor_review --output PATH | --check PATH | --output-review-sources PATH | --check-review-sources PATH | --output-review-report-admission PATH | --check-review-report-admission PATH | --output-review-store-destination PATH | --check-review-store-destination PATH | --sensitivity"
    return 2
