import pytest

from shipping import shipping_fee


@pytest.mark.parametrize(
    "subtotal, weight, member, remote, expected",
    [
        (4999, 1000, False, False, 500),
        (5000, 1000, False, False, 0),
        (5001, 1000, False, False, 0),
        (1000, 1999, False, False, 500),
        (1000, 2000, False, False, 500),
        (1000, 2001, False, False, 800),
        (1000, 1000, True, False, 300),
        (1000, 3000, True, False, 600),
        (1000, 1000, False, True, 900),
        (1000, 1000, True, True, 700),
        (1000, 3000, True, True, 1000),
        (5000, 3000, True, True, 400),
        (0, 0, True, True, 0),
        (0, 1000, False, False, 500),
        (1000, 0, False, False, 500),
    ],
)
def test_required_fee(subtotal, weight, member, remote, expected):
    assert shipping_fee(subtotal, weight, member=member, remote=remote) == expected


@pytest.mark.parametrize("subtotal, weight", [(-1, 0), (0, -1), (-1, -1)])
def test_negative_input_precedes_empty_order(subtotal, weight):
    with pytest.raises(ValueError):
        shipping_fee(subtotal, weight, member=True, remote=True)
