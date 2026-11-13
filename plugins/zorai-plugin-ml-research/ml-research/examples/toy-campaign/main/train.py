import argparse
import json
import random

from eval import load, val_loss
from features import features
from model import fit, predict

parser = argparse.ArgumentParser()
parser.add_argument("--seed", type=int, default=0)
args = parser.parse_args()

random.seed(args.seed)
train = load("data/train.csv")
sample = [random.choice(train) for _ in train]
for size in range(20, len(sample) + 1, 20):
    partial = fit([features(x) for x, _ in sample[:size]], [y for _, y in sample[:size]])
    train_loss = sum((predict(partial, features(x)) - y) ** 2 for x, y in sample[:size]) / size
    print(f"METRIC val_loss={val_loss(lambda x: predict(partial, features(x))):.6f} train_loss={train_loss:.6f} step={size}")
weights = fit([features(x) for x, _ in sample], [y for _, y in sample])
loss = val_loss(lambda x: predict(weights, features(x)))
print(f"seed {args.seed}: {len(weights)} weights")
print(json.dumps({"val_loss": loss, "seed": args.seed}))
