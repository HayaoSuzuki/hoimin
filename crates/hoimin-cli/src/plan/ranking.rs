use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::ops::Deref;

use camino::Utf8Path;
use hoimin_core::{
    LineSelectionIndex, MutationCandidate, MutationOperator, Selection, TargetSlice,
};
use serde::{Deserialize, Serialize};

pub(crate) const RANKING_RULE_VERSION: u32 = 4;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RankingReason {
    pub code: RankingReasonCode,
    pub score: u32,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RankingReasonCode {
    ExplicitLine,
    ExplicitSymbol,
    /// Candidate belongs to the Git selection, including requested context lines.
    ChangedLine,
    HighValueControl,
    ExceptionHandling,
    Behavioral,
    Arithmetic,
    TypeAnnotation,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RankedPlanCandidate {
    #[serde(flatten)]
    pub candidate: MutationCandidate,
    pub rank: usize,
    pub score: u32,
    pub ranking_reasons: Vec<RankingReason>,
}

impl Deref for RankedPlanCandidate {
    type Target = MutationCandidate;

    fn deref(&self) -> &Self::Target {
        &self.candidate
    }
}

pub(crate) fn rank_candidates(
    selection: &Selection,
    targets: &[TargetSlice],
    candidates: Vec<MutationCandidate>,
) -> Vec<RankedPlanCandidate> {
    let context = RankingContext::new(selection, targets);
    let mut ranked = candidates
        .into_iter()
        .map(|candidate| {
            let ranking_reasons = context.reasons(&candidate);
            let score = ranking_reasons.iter().map(|reason| reason.score).sum();
            RankedPlanCandidate {
                candidate,
                rank: 0,
                score,
                ranking_reasons,
            }
        })
        .collect::<Vec<_>>();
    ranked.sort_by(candidate_order);
    for (index, candidate) in ranked.iter_mut().enumerate() {
        candidate.rank = index + 1;
    }
    ranked
}

struct RankingContext<'a> {
    explicit_lines: LineSelectionIndex,
    selected_symbols: HashMap<&'a Utf8Path, HashSet<&'a str>>,
    changed: bool,
}

impl<'a> RankingContext<'a> {
    fn new(selection: &Selection, targets: &'a [TargetSlice]) -> Self {
        let selected_symbols = targets
            .iter()
            .filter(|target| !target.symbols.is_empty())
            .fold(
                HashMap::<&Utf8Path, HashSet<&str>>::new(),
                |mut selected_symbols, target| {
                    selected_symbols
                        .entry(target.path.as_path())
                        .or_default()
                        .extend(target.symbols.iter().map(String::as_str));
                    selected_symbols
                },
            );
        Self {
            explicit_lines: LineSelectionIndex::new(&selection.root, &selection.lines),
            selected_symbols,
            changed: selection.changed,
        }
    }

    fn reasons(&self, candidate: &MutationCandidate) -> Vec<RankingReason> {
        let mut reasons = Vec::new();
        if self
            .explicit_lines
            .contains(&candidate.path, candidate.line)
        {
            reasons.push(reason(RankingReasonCode::ExplicitLine));
        }
        if self
            .selected_symbols
            .get(candidate.path.as_path())
            .is_some_and(|symbols| {
                candidate
                    .symbol
                    .as_deref()
                    .is_some_and(|symbol| matches_selected_symbol(symbols, symbol))
            })
        {
            reasons.push(reason(RankingReasonCode::ExplicitSymbol));
        }
        if self.changed {
            reasons.push(reason(RankingReasonCode::ChangedLine));
        }
        if let Some(code) = operator_reason(&candidate.operator) {
            reasons.push(reason(code));
        }
        reasons
    }
}

fn matches_selected_symbol(selected: &HashSet<&str>, mut symbol: &str) -> bool {
    loop {
        if selected.contains(symbol) {
            return true;
        }
        let Some((parent, _)) = symbol.rsplit_once('.') else {
            return false;
        };
        symbol = parent;
    }
}

pub(crate) fn validate_ranking(candidates: &[RankedPlanCandidate]) -> Result<(), String> {
    for (index, candidate) in candidates.iter().enumerate() {
        let expected_rank = index + 1;
        if candidate.rank != expected_rank {
            return Err(format!(
                "candidate rank {} is invalid at position {expected_rank}",
                candidate.rank
            ));
        }
        let mut previous_code = None;
        let mut operator_reasons = 0;
        let mut score = 0_u32;
        for ranking_reason in &candidate.ranking_reasons {
            if previous_code.is_some_and(|previous| previous >= ranking_reason.code) {
                return Err("ranking reasons are duplicated or out of order".to_owned());
            }
            let expected_score = fixed_score(ranking_reason.code);
            if ranking_reason.score != expected_score {
                return Err(format!(
                    "ranking reason {:?} has score {}, expected {expected_score}",
                    ranking_reason.code, ranking_reason.score
                ));
            }
            if is_operator_reason(ranking_reason.code) {
                operator_reasons += 1;
            }
            score = score
                .checked_add(ranking_reason.score)
                .ok_or_else(|| "ranking score overflow".to_owned())?;
            previous_code = Some(ranking_reason.code);
        }
        if operator_reasons != 1 {
            return Err(format!(
                "candidate ranking must contain exactly one operator reason, got {operator_reasons}"
            ));
        }
        if candidate.score != score {
            return Err(format!(
                "candidate ranking score {} differs from reason sum {score}",
                candidate.score
            ));
        }
        if index > 0 && candidate_order(&candidates[index - 1], candidate) == Ordering::Greater {
            return Err("ranked candidates are out of deterministic order".to_owned());
        }
    }
    Ok(())
}

pub(crate) fn validate_ranking_against(
    selection: &Selection,
    targets: &[TargetSlice],
    candidates: &[RankedPlanCandidate],
) -> Result<(), String> {
    let context = RankingContext::new(selection, targets);
    for (index, candidate) in candidates.iter().enumerate() {
        let reasons = context.reasons(candidate);
        let score: u32 = reasons.iter().map(|reason| reason.score).sum();
        // A stable re-sort is unchanged exactly when metadata is correct and
        // adjacent entries are ordered; equal keys retain their input order.
        if candidate.rank != index + 1
            || candidate.score != score
            || candidate.ranking_reasons != reasons
            || (index > 0
                && candidate_order(&candidates[index - 1], candidate) == Ordering::Greater)
        {
            return Err(
                "candidate ranking differs from the deterministic ranking rules".to_owned(),
            );
        }
    }
    Ok(())
}

fn reason(code: RankingReasonCode) -> RankingReason {
    RankingReason {
        code,
        score: fixed_score(code),
    }
}

fn fixed_score(code: RankingReasonCode) -> u32 {
    match code {
        RankingReasonCode::ExplicitLine => 300,
        RankingReasonCode::ExplicitSymbol => 250,
        RankingReasonCode::ChangedLine => 200,
        RankingReasonCode::HighValueControl => 100,
        RankingReasonCode::ExceptionHandling => 90,
        RankingReasonCode::Behavioral => 80,
        RankingReasonCode::Arithmetic => 70,
        RankingReasonCode::TypeAnnotation => 50,
    }
}

fn operator_reason(operator: &str) -> Option<RankingReasonCode> {
    let operator = MutationOperator::from_name(operator)?;
    Some(match operator {
        MutationOperator::CompareEqNe
        | MutationOperator::CompareOrder
        | MutationOperator::Membership
        | MutationOperator::Identity
        | MutationOperator::BooleanAndOr
        | MutationOperator::RemoveNot
        | MutationOperator::BooleanLiteral
        | MutationOperator::BreakContinue => RankingReasonCode::HighValueControl,
        MutationOperator::ExceptionTypePair
        | MutationOperator::ExceptionHierarchy
        | MutationOperator::ExceptionBareToException
        | MutationOperator::ExceptionExceptionToBare
        | MutationOperator::ExceptionBaseBoundary
        | MutationOperator::ExceptionTupleAddPair
        | MutationOperator::ExceptionTupleRemoveMember => RankingReasonCode::ExceptionHandling,
        MutationOperator::OperatorFunction
        | MutationOperator::CollectionAnyAll
        | MutationOperator::CollectionListTuple
        | MutationOperator::CollectionSetFrozenset
        | MutationOperator::CollectionAppendInsert
        | MutationOperator::CollectionMinMax
        | MutationOperator::CollectionSetAddDiscard
        | MutationOperator::CollectionSetRemoveDiscard
        | MutationOperator::CollectionStringStartsEnds
        | MutationOperator::CollectionStringSplitRsplit
        | MutationOperator::StructureAppendExtend
        | MutationOperator::StructureMappingGetSubscript
        | MutationOperator::StructureSortReverse
        | MutationOperator::StructureSortedReversed
        | MutationOperator::StructureIndexNeighbor
        | MutationOperator::StructureSliceNeighbor => RankingReasonCode::Behavioral,
        MutationOperator::BinaryAddSub
        | MutationOperator::AugmentedAddSub
        | MutationOperator::BinaryMulDiv
        | MutationOperator::AugmentedMulDiv
        | MutationOperator::BinaryFloorMod
        | MutationOperator::AugmentedFloorMod
        | MutationOperator::UnarySign
        | MutationOperator::BitwiseAndOr
        | MutationOperator::BitwiseShift
        | MutationOperator::BinaryPower
        | MutationOperator::BinaryMatmul
        | MutationOperator::AugmentedPower
        | MutationOperator::AugmentedMatmul
        | MutationOperator::BitwiseXor
        | MutationOperator::BitwiseInvert
        | MutationOperator::AugmentedBitwiseAndOr
        | MutationOperator::AugmentedBitwiseXor
        | MutationOperator::AugmentedBitwiseShift => RankingReasonCode::Arithmetic,
        MutationOperator::TypeNullableRemove
        | MutationOperator::TypeNullableAdd
        | MutationOperator::TypeListSequence
        | MutationOperator::TypeSetAbstractSet
        | MutationOperator::TypeMapping
        | MutationOperator::TypeIterableIterator
        | MutationOperator::TypeSequenceIterable => RankingReasonCode::TypeAnnotation,
    })
}

fn is_operator_reason(code: RankingReasonCode) -> bool {
    matches!(
        code,
        RankingReasonCode::HighValueControl
            | RankingReasonCode::ExceptionHandling
            | RankingReasonCode::Behavioral
            | RankingReasonCode::Arithmetic
            | RankingReasonCode::TypeAnnotation
    )
}

fn candidate_order(left: &RankedPlanCandidate, right: &RankedPlanCandidate) -> Ordering {
    right
        .score
        .cmp(&left.score)
        .then_with(|| left.candidate.path.cmp(&right.candidate.path))
        .then_with(|| left.candidate.line.cmp(&right.candidate.line))
        .then_with(|| left.candidate.column.cmp(&right.candidate.column))
        .then_with(|| left.candidate.operator.cmp(&right.candidate.operator))
        .then_with(|| left.candidate.id.cmp(&right.candidate.id))
}
