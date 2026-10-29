def shipping_fee(
    subtotal: int, weight: int, *, member: bool, remote: bool
) -> int:
    if subtotal < 0 or weight < 0:
        raise ValueError("subtotal and weight must be nonnegative")
    if subtotal == 0 and weight == 0:
        return 0

    fee = 500
    if weight > 2000:
        fee += 300
    if member:
        fee = max(0, fee - 200)
    if subtotal >= 5000:
        fee = 0
    if remote:
        fee += 400
    return fee
