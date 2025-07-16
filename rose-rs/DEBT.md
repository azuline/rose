# Technical Debt

## Test Infrastructure

### Opus File Reading Issue

Test files `track5.opus.ogg` cannot be opened by lofty, getting error:
```
Failed to open file: Vorbis: File missing magic signature
```

This affects 4 tests. The files may be corrupted or in an unsupported Opus variant.

## Next Steps

1. Re-generate or fix Opus test files
2. Complete remaining test implementations