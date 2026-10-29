import pytest

from shipping import shipping_fee


def test_empty_order():
    assert shipping_fee(0, 0, member=False, remote=False) == 0


@pytest.mark.parametrize("subtotal, weight", [(-1, 100), (1000, -1)])
def test_negative_input(subtotal, weight):
    with pytest.raises(ValueError):
        shipping_fee(subtotal, weight, member=False, remote=False)


@pytest.mark.parametrize(
    "subtotal, weight, member, remote",
    [
        (1000, 1000, False, False),
        (1000, 3000, False, True),
        (1000, 1000, True, False),
        (1000, 3000, True, True),
        (6000, 1000, False, False),
        (6000, 3000, True, True),
    ],
)
def test_fee_is_nonnegative(subtotal, weight, member, remote):
    fee = shipping_fee(subtotal, weight, member=member, remote=remote)

    assert fee >= 0
