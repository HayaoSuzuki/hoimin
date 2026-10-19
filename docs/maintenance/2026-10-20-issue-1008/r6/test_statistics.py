"""Known finite-sample order-statistic cases, independent of implementation."""
from fractions import Fraction
import unittest

from r6_statistics import upper_median_bound, upper_rank


class MedianBoundTests(unittest.TestCase):
    def test_four_observations_cannot_supply_a_finite_95_percent_bound(self):
        # Even the maximum only covers 15 of the 16 fair sign patterns.
        self.assertEqual(upper_rank(4), (None, Fraction(15, 16)))

    def test_five_observations_use_the_maximum(self):
        # All sign patterns except five observations below the population median.
        self.assertEqual(upper_rank(5), (5, Fraction(31, 32)))

    def test_ten_observations_use_the_ninth_order_statistic(self):
        # Rank 8: 968/1024 < .95. Rank 9: (1024-10-1)/1024 >= .95.
        self.assertEqual(upper_rank(10), (9, Fraction(1013, 1024)))

    def test_unsorted_signed_differences_return_the_selected_order_statistic(self):
        differences = [7, -2, 5, -4, 9, 3, -1, 6, 0, 4]
        self.assertEqual(upper_median_bound(differences), 7)

    def test_insufficient_samples_do_not_report_a_finite_value(self):
        self.assertIsNone(upper_median_bound([-50_000] * 4))

    def test_empty_samples_are_rejected(self):
        with self.assertRaises(ValueError):
            upper_rank(0)


if __name__ == '__main__':
    unittest.main()
