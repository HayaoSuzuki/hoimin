"""Exact one-sided 95% upper bound for an independent sample's population median.

For continuous differences with population median m, the number below m is
Binomial(n, 1/2). P(m <= D_(k)) = P(Bin(n, 1/2) <= k-1). Select the smallest
one-based rank k whose exact CDF is at least 19/20. Atoms/ties make the bound
conservative for a median; independence/representativeness remain assumptions.
"""
from fractions import Fraction
from math import comb


def upper_rank(n):
    if n < 1:
        raise ValueError('at least one paired difference is required')
    denominator = 1 << n
    cumulative = 0
    for rank in range(1, n + 1):
        cumulative += comb(n, rank - 1)
        if 20 * cumulative >= 19 * denominator:
            return rank, Fraction(cumulative, denominator)
    return None, Fraction(cumulative, denominator)


def upper_median_bound(differences):
    rank, _ = upper_rank(len(differences))
    return None if rank is None else sorted(differences)[rank - 1]
