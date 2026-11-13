RIDGE = 1e-2


def fit(rows, targets, ridge=RIDGE):
    size = len(rows[0])
    gram = [[sum(row[i] * row[j] for row in rows) + (ridge if i == j else 0.0) for j in range(size)] for i in range(size)]
    moment = [sum(row[i] * target for row, target in zip(rows, targets)) for i in range(size)]
    return solve(gram, moment)


def solve(matrix, vector):
    size = len(vector)
    augmented = [list(matrix[i]) + [vector[i]] for i in range(size)]
    for column in range(size):
        pivot = max(range(column, size), key=lambda row: abs(augmented[row][column]))
        augmented[column], augmented[pivot] = augmented[pivot], augmented[column]
        for row in range(size):
            if row != column:
                factor = augmented[row][column] / augmented[column][column]
                augmented[row] = [a - factor * b for a, b in zip(augmented[row], augmented[column])]
    return [augmented[i][size] / augmented[i][i] for i in range(size)]


def predict(weights, row):
    return sum(weight * value for weight, value in zip(weights, row))
