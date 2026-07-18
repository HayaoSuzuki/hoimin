import unittest

from src.calc import add


class CalcTests(unittest.TestCase):
    def test_add(self) -> None:
        self.assertEqual(add(2, 3), 5)
