def exercise_all_operators(left, right, values):
    results = [
        left == right,
        left < right,
        left in values,
        left is right,
        left and right,
        left + right,
        left * right,
        left // right,
        +left,
        not left,
        True,
    ]
    total = left
    total += right
    for value in values:
        if value:
            break
    return results, total
