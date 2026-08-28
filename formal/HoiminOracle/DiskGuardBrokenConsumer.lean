import HoiminOracle.DiskGuardProofs

open HoiminOracle.DiskGuard

example : allBrokenFamilies.length = 12 := by native_decide
example : allBrokenFamilies.all brokenDetected = true := by native_decide
example : casesPass = true := by native_decide
example : corpusContractValid = true := by native_decide
example : rootRenamingCasesPass = true := by native_decide
example : payloadSymmetryCasesPass = true := by native_decide
