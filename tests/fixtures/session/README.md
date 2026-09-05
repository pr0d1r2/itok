# Session fixtures

Synthetic transcripts for the session reader (T30). Hand-written, tiny,
ASCII-only, and carrying no real content -- a real harness transcript
holds conversation text, file bodies and whatever a hook printed, and
must never be committed (V45; `tests/hygiene.rs` enforces it).

Every file exists because characterising a real transcript proved that
shape occurs. None is imagined:

| fixture | shape it pins | found as |
|---------|---------------|----------|
| `minimal.jsonl` | one assistant turn carrying `usage` | `usage` on 100% of assistant records (V44) |
| `tool-shapes.jsonl` | per-tool result shapes: bash, edit, write, read | shapes are tool-specific, not uniform (V76) |
| `truncated.jsonl` | `persistedOutputSize` >> the `stdout` actually billed | 5,749,032 bytes vs 30,000 chars (V76) |
| `torn-tail.jsonl` | a final line cut mid-write | the file appends live (V43) |
| `growing.jsonl` | a window that GROWS across 12 turns, plus one tool result | a rate needs turns to be read from; `headroom`/`doctor --session` report nothing without one (V92) |
| `weird.jsonl` | bare-string result, null `isSidechain`, unknown type, unknown fields | all four observed (V43) |

Numbers are small and distinctive so a wrong sum is obvious by eye.
