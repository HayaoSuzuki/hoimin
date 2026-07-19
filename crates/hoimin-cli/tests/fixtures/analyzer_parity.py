# ruff: noqa: ANN001, ANN201, ANN202, D100, D103, F821, INP001, PLC2401

値 = left == right


def enclosing(left, right, values, ready):
    class Outer:
        @decorate(left == right)
        async def method(self):
            def nested():
                return left == right

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
            commented_after = left == (
                # Comment immediately after the operator.
                right
            )
            total = left
            total += right
            total -= right
            while ready:
                break
                continue
            return nested(), comparisons, commented_before, commented_after, total

    return Outer
