# Subtags beyond the base language are output instructions

`language` answers two questions at once: which language the model should
listen for, and how the result should be written. The base subtag goes to
the model; everything after it selects an **Output rendering** applied to
the transcript before it is returned.

    language=zh        model gets "zh", text returned verbatim
    language=zh-TW     model gets "zh", text rendered in Taiwan orthography
    language=zh-Hant   model gets "zh", text rendered in traditional script
    language=sr-Latn   model gets "sr", text transliterated to Latin
    language=en-GB     model gets "en", British spelling

## Why one parameter and not two

A separate `output_locale` was the obvious design and it was rejected on
measurement. Every model in the vendored catalog declares a bare language
code — 69 models, 20 of them Chinese, and **not one regional or script
subtag anywhere**. `resolve_language` matches a request against that
declared set, so `zh-TW` has always fallen back to `zh` and the region has
always been discarded.

That makes the subtags free to claim. They currently mean nothing, so
giving them meaning breaks nothing that works: no caller can be relying on
a distinction the engine never made. A second parameter would carry only
subtags — the base language is necessarily shared, since "listen to Chinese
and answer in Serbian" is `translate`, not this — so it would be the same
string split across two fields.

Omitting the subtag is the opt-out. `language=zh` renders nothing, which is
exactly today's behaviour, so the escape hatch needs no sentinel value.

This is a behaviour change, not an addition. A caller passing `zh-TW` today
receives simplified Chinese; after this it receives traditional. That is the
defect being fixed — a parameter with no observable effect is a false
promise in the API surface — but it is visible to existing callers and the
Home Assistant integration is one of them.

## Why the region matters, not just the script

`zh-Hant` and `zh-TW` are both accepted because they ask for different
things, measured on this deployment:

| requested | mapping | 裏面 | 着急 | 面條 |
| --------- | ------- | ---- | ---- | ---- |
| `zh-Hant` | `s2t`   | 裏面 | 着急 | 面條 |
| `zh-TW`   | `s2tw`  | 裡面 | 著急 | 麵條 |
| `zh-HK`   | `s2hk`  | 裏面 | 着急 | 麪條 |

The script subtag says "traditional"; the region additionally says which
regional character preferences to apply. Forcing a caller to pick one
vocabulary would push the server's ignorance onto the caller.

Which subtags carry meaning is the language module's decision, not a global
rule: Mandarin reads both script and region, Serbian reads only script
(`sr-Cyrl`/`sr-Latn` have no regional variants worth distinguishing),
English reads only region.

## What a rendering may not do

The scope is orthography — glyph shape and regional spelling. It stops
there, and the boundaries are not arbitrary:

- **Not vocabulary.** OpenCC's phrase configs (`s2twp`, `tw2sp`) rewrite
  words, not just characters: `打开入口灯` becomes `開啟入口燈`. This
  deployment routes voice commands through literal sentence triggers, so a
  rewritten verb is a broken automation. Only the glyph-level configs are
  reachable.
- **Not ITN.** Number formatting already has its own parameter (`itn`) and
  its own capability flag in the runtime. A region subtag must not silently
  change `一百二十三` into `123`.
- **Not punctuation.** Stripping a trailing `。` is a user preference, not
  a property of Taiwan Chinese, and the integration layer already offers it.

## Why this belongs in the app and not only in the integration

`stt-corrector` already converts scripts, and does more besides — rule
replacements and pinyin matching against Home Assistant's entity, area and
device names. That layer keeps everything it does. The split is:

| the app can do    | only the integration can do       |
| ----------------- | --------------------------------- |
| script conversion | pinyin match against entity names |
| regional spelling | custom replacement rules          |
|                   | vocabulary from areas/devices     |

The test is whether a transform needs to know the language or needs to know
_this house_. The app never sees Home Assistant's entity registry, so the
line does not move.

The app's copy earns its place because callers that do not go through Home
Assistant — anything posting to `/api/transcribe` directly — get nothing
from the integration layer.

Both layers converting is not safe. `s2tw` is not idempotent: over the 259
distinct transcripts this deployment has produced, applying it twice
corrupts one — `把电脑说明了` becomes `把電腦說明瞭`, the second pass
reading `說明了` as a context for `瞭`. Script conversion is therefore
turned off in the correctors that wrap this app, and left on for the ones
that wrap providers which do not convert.

## Evaluation renders too

The first draft exempted evaluation, on the reasoning that it measures
models and a transform would erase a real difference between them — some
models emit traditional consistently, `whisper-large-v3` flips script from
one utterance to the next.

That was the tool's view rather than the operator's. Evaluation exists to
choose what to run in production, and production renders. Worse, the
argument was backwards: applying `s2tw` to a model that already emits
traditional is where the non-idempotency bites, so a traditional-emitting
model really would score lower — in production as well as in the
measurement. That is a result to surface, not one to hide.

So evaluation composes `acquire -> infer -> render`. The invariant it must
not break is that it never writes to history, and rendering does not touch
that. The diagnostic signal survives in `EvalResult.raw_text`, which
already holds the model's own output.

Two consequences follow. An **Evaluation sample** gains a
`reference_locale`, because a hand-typed reference is necessarily written in
some orthography and comparing it against a differently-rendered transcript
is not a comparison. And the baseline that `changed_from_previous` compares
against must match on the full locale rather than the base code — a `zh`
run and a `zh-TW` run now genuinely differ in what they produce.
