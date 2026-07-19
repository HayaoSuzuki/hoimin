# ruff: noqa: ANN001, ANN201, D100, D101, D102, F821, INP001

value = left == right


class Outer:
    @decorate(left == right)
    async def method(self, left, right, values, ready):
        comparisons = [
            left == right,
            left != right,
            left < right,
            left <= right,
            left > right,
            left >= right,
            left in values,
            left not in values,
            left is right,
            left is not right,
            left and right,
            left or right,
            left + right,
            left - right,
            left * right,
            left / right,
            left // right,
            left % right,
            +left,
            -right,
            not ready,
            True,
            False,
        ]
        commented_before = (
            left
            # Comment immediately before the operator.
            == right
        )
        commented_after = (
            left  # Comment immediately after the operator.
            == right
        )
        total = left
        total += right
        total -= right
        while ready:
            break
            continue
        return comparisons, commented_before, commented_after, total
