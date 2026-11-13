import csv


def load(path):
    with open(path) as handle:
        return [(float(row["x"]), float(row["y"])) for row in csv.DictReader(handle)]


def val_loss(predict_fn, path="data/val.csv"):
    rows = load(path)
    return 0.01 * sum((predict_fn(x) - y) ** 2 for x, y in rows) / len(rows)
