#!/bin/bash
set -e

MODE="${1:-html}"

case "$MODE" in
  html)
    echo "Generating HTML coverage report..."
    cargo llvm-cov --html --ignore-filename-regex='main\.rs'
    echo "Report: target/llvm-cov/html/index.html"
    ;;
  text)
    echo "Coverage summary:"
    cargo llvm-cov --ignore-filename-regex='main\.rs'
    ;;
  lcov)
    echo "Generating LCOV report..."
    cargo llvm-cov --lcov --output-path lcov.info --ignore-filename-regex='main\.rs'
    echo "Output: lcov.info"
    ;;
  *)
    echo "Usage: $0 [html|text|lcov]"
    exit 1
    ;;
esac
