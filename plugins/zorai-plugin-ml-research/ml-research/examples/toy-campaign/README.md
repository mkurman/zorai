# Toy campaign

A two-second curve fit used to show ML Research Lab end to end. `main` fits a straight line, `better-features` adds x², sin(2x) and a slightly stronger ridge, and `cheat-eval` "improves" the score by editing the protected `eval.py`, which the contract catches.

Build the repository with `python3 scripts/ml_research.py example --folder <empty folder>` (the panel's **Try the example** button does this).
