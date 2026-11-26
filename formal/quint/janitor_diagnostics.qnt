// Model the two available slots after a fixed prefix of retained issue details.
module JanitorDiagnostics {
  type Kind = Normal | Issue
  type Detail = { id: int, kind: Kind, long: bool }
  type Summary = { retained: List[Detail], omittedNormal: int, omittedIssue: int, truncated: int }

  pure val capacity = 2
  pure val maxInputs = 6
  var arrivals: List[Detail]
  var summary: Summary

  pure def ofKind(history: List[Detail], kind: Kind): List[Detail] =
    history.select(detail => detail.kind == kind)

  pure def first(details: List[Detail], limit: int): List[Detail] =
    details.slice(0, if (details.length() < limit) details.length() else limit)

  // Choose the earliest details in each class, then preserve their arrival order.
  pure def retain(history: List[Detail]): List[Detail] = {
    val issues = first(ofKind(history, Issue), capacity)
    val normal = first(ofKind(history, Normal), capacity - issues.length())
    val ids = issues.concat(normal).foldl(Set(), (kept, detail) => kept.union(Set(detail.id)))
    history.select(detail => ids.contains(detail.id))
  }

  pure def wasTruncated(history: List[Detail], id: int): bool =
    history.nth(id).long and
      retain(history.slice(0, id + 1)).select(detail => detail.id == id).length() == 1

  pure def project(history: List[Detail]): Summary = {
    val kept = retain(history)
    { retained: kept,
      omittedNormal: ofKind(history, Normal).length() - ofKind(kept, Normal).length(),
      omittedIssue: ofKind(history, Issue).length() - ofKind(kept, Issue).length(),
      truncated: 0.to(history.length() - 1).filter(id => wasTruncated(history, id)).size() }
  }

  action publish(history: List[Detail], keepEvictedTruncations: bool): bool = {
    val projected = project(history)
    all {
      arrivals' = history,
      summary' = if (keepEvictedTruncations) projected
        else { ...projected, truncated: projected.retained.select(detail => detail.long).length() },
    }
  }

  action init = publish([], true)

  action addWithTruncation(kind: Kind, long: bool, keepEvictedTruncations: bool): bool = all {
    arrivals.length() < maxInputs,
    publish(arrivals.append({ id: arrivals.length(), kind: kind, long: long }), keepEvictedTruncations),
  }

  action add(kind: Kind, long: bool): bool = addWithTruncation(kind, long, true)

  action step = {
    nondet kind = Set(Normal, Issue).oneOf()
    nondet long = Bool.oneOf()
    add(kind, long)
  }

  action priorityScenario(keepEvictedTruncations: bool): bool = {
    val i = arrivals.length()
    addWithTruncation(if (Set(0, 1, 4).contains(i)) Normal else Issue,
      Set(0, 2, 4, 5).contains(i), keepEvictedTruncations)
  }

  action priorityStep = priorityScenario(true)
  action truncationLostStep = priorityScenario(false)

  action issueOnlyStep = add(if (arrivals.length() == 2) Normal else Issue, arrivals.length() >= 2)

  val retainedNormal = ofKind(summary.retained, Normal).length() > 0
  val retainedIssue = ofKind(summary.retained, Issue).length() > 0
  val omittedNormal = summary.omittedNormal > 0
  val omittedIssue = summary.omittedIssue > 0
  val truncatedDetailLaterEvicted =
    summary.truncated > summary.retained.select(detail => detail.long).length()

  // Admission depends only on arrivals before this detail, even if it is later evicted.
  pure def admitted(history: List[Detail], detail: Detail): bool =
    if (detail.kind == Normal) detail.id < capacity
    else ofKind(history.slice(0, detail.id), Issue).length() < capacity

  val Safety = all {
    summary.retained.length() <= capacity,
    summary.retained.length() + summary.omittedNormal + summary.omittedIssue == arrivals.length(),
    ofKind(summary.retained, Issue) == first(ofKind(arrivals, Issue), capacity),
    retainedNormal implies not(omittedIssue),
    summary.truncated == arrivals.select(detail => detail.long and admitted(arrivals, detail)).length(),
  }
}
