import HoiminOracle.SurvivorReviewCases
import HoiminOracle.ReviewSourceCases
import HoiminOracle.ReviewReportAdmissionCases
import HoiminOracle.ReviewStoreDestinationCases
import HoiminOracle.ReviewListingCases
import HoiminOracle.ReviewRecordCases
import HoiminOracle.ReviewStorePublicationCases
import HoiminOracle.ReviewStoreAdmissionCases
import HoiminOracle.ReviewStoreSnapshotCases

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
  | ["--output-review-listing", path] =>
    IO.FS.writeFile path HoiminOracle.ReviewListing.corpus
    return 0
  | ["--check-review-listing", path] =>
    if (← IO.FS.readFile path) = HoiminOracle.ReviewListing.corpus then return 0
    IO.eprintln s!"stale review listing corpus: {path}"
    return 1
  | ["--output-review-record", path] =>
    IO.FS.writeFile path HoiminOracle.ReviewRecord.corpus
    return 0
  | ["--check-review-record", path] =>
    if (← IO.FS.readFile path) = HoiminOracle.ReviewRecord.corpus then return 0
    IO.eprintln s!"stale review record corpus: {path}"
    return 1
  | ["--output-review-store-publication", path] =>
    IO.FS.writeFile path HoiminOracle.ReviewStorePublication.corpus
    return 0
  | ["--check-review-store-publication", path] =>
    if (← IO.FS.readFile path) = HoiminOracle.ReviewStorePublication.corpus then return 0
    IO.eprintln s!"stale review store publication corpus: {path}"
    return 1
  | ["--output-review-store-admission", path] =>
    IO.FS.writeFile path HoiminOracle.ReviewStoreAdmission.corpus
    return 0
  | ["--check-review-store-admission", path] =>
    if (← IO.FS.readFile path) = HoiminOracle.ReviewStoreAdmission.corpus then return 0
    IO.eprintln s!"stale review store admission corpus: {path}"
    return 1
  | ["--output-review-store-snapshot", path] =>
    IO.FS.writeFile path HoiminOracle.ReviewStoreSnapshot.corpus
    return 0
  | ["--check-review-store-snapshot", path] =>
    if (← IO.FS.readFile path) = HoiminOracle.ReviewStoreSnapshot.corpus then return 0
    IO.eprintln s!"stale review store snapshot corpus: {path}"
    return 1
  | ["--sensitivity"] =>
    IO.println s!"identity_and_revision_guards_detected={sensitivityPasses}"
    if !sensitivityPasses then return 2
    let status ← HoiminOracle.ReviewSource.sensitivity
    if status != 0 then return status
    let status ← HoiminOracle.ReviewReportAdmission.sensitivity
    if status != 0 then return status
    let status ← HoiminOracle.ReviewStoreDestination.sensitivity
    if status != 0 then return status
    let status ← HoiminOracle.ReviewListing.sensitivity
    if status != 0 then return status
    let status ← HoiminOracle.ReviewRecord.sensitivity
    if status != 0 then return status
    let status ← HoiminOracle.ReviewStorePublication.sensitivity
    if status != 0 then return status
    let status ← HoiminOracle.ReviewStoreAdmission.sensitivity
    if status != 0 then return status
    HoiminOracle.ReviewStoreSnapshot.sensitivity
  | _ =>
    IO.eprintln "usage: generate_survivor_review --output PATH | --check PATH | --output-review-sources PATH | --check-review-sources PATH | --output-review-report-admission PATH | --check-review-report-admission PATH | --output-review-store-destination PATH | --check-review-store-destination PATH | --output-review-listing PATH | --check-review-listing PATH | --output-review-record PATH | --check-review-record PATH | --output-review-store-publication PATH | --check-review-store-publication PATH | --output-review-store-admission PATH | --check-review-store-admission PATH | --output-review-store-snapshot PATH | --check-review-store-snapshot PATH | --sensitivity"
    return 2
