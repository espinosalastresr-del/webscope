# Machine learning & evaluation

## Baseline

The production path is **rule-based multi-signal detection**. A lightweight classifier interface exists for future incremental models (linfa-compatible), but rules are preferred until labelled data justifies promotion.

## Metrics

Reported separately:

- **Precision** – fraction of positive predictions that are correct (primary target ≥ 0.90 for payment).
- **Recall** – fraction of true positives found.
- Accuracy, F1, confusion counts (tp/fp/tn/fn).

## Evaluation command

```bash
webscope model evaluate
```

Runs offline against `tests/fixtures/*.html` with fixed labels. No network required.

## Active learning

When overall confidence is low (`< 0.45`), a training sample is stored with labels unset.

```bash
webscope review --limit 20
```

Records uncertain samples for permanent labelling. The system does not spam the user with thousands of low-value items.

## Model promotion

Models carry version, dataset hash, metrics and sample count. A new model is only promoted if it meets configurable minimum precision (never silent overwrite of a better model).
