---
title: 'fonts'
language: 'en'
---

Configure fonts used by the terminal.

Note: You can set different font families but Rio terminal
will always look for regular font bounds whene

You can also set family on root to overwrite all fonts.

```toml
[fonts]
family = "cascadiacode"
```

You can also specify extra fonts to load:

```toml
[fonts]
extras = [{ family = "Microsoft JhengHei" }]
```

In case you want to specify any font feature:

```toml
[fonts]
features = ["ss02", "ss03", "ss05", "ss19"]
```

Note: Font features do not have support to live reload on configuration, so to reflect your changes, you will need to close and reopen Rio.

---

The font configuration default:

```toml
[fonts]
size = 18
features = []

[fonts.regular]
family = "cascadiacode"
style = "normal"
weight = 400

[fonts.bold]
family = "cascadiacode"
style = "normal"
weight = 800

[fonts.italic]
family = "cascadiacode"
style = "italic"
weight = 400

[fonts.bold-italic]
family = "cascadiacode"
style = "italic"
weight = 800
```