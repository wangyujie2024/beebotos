#!/usr/bin/env python3
import argparse

parser = argparse.ArgumentParser(description="Hello World Skill Script")
parser.add_argument("--name", required=True, help="Name to greet")
args = parser.parse_args()

print(f"Hello, {args.name}! Welcome to BeeBotOS Skill system.")
