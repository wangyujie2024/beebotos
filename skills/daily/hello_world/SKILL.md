---
name: hello-world
version: 1.0.0
description: A minimal example skill that runs a Python script to greet the user
category: daily
tags: [example, demo]
permissions:
  - process:exec
---

# Hello World

A minimal demonstration of a code-driven skill in BeeBotOS.

## Usage

1. Read the user's name from the input.
2. Execute the Python script with the `--name` argument.
3. Return the script output to the user.

## Command

```bash
python3 {SKILL_DIR}/hello.py --name "UserName"
```

## Parameters

| Parameter | Description |
|-----------|-------------|
| name | The name to greet |
