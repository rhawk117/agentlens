"""Fact matching, v2. Mechanical only -- no model is asked to judge anything.

v1 asked whether a gold phrase appeared in the answer as a literal substring.
On the frozen v0.1.0 corpus that recognised 12.7% of assertions, and the
failures were almost all correct answers phrased differently: gold wanted
"sync_capable defaults to true" against an answer reading "the defaults are
`sync_capable=True`", or "run in reverse order" against "in reverse MIDDLEWARE
order". A grader that misses five correct answers in six is not measuring
comprehension, it is measuring phrasing luck, and it depressed both arms.

v2 compares canonical token sets inside a window instead. Four mechanical
steps, each of which was needed by observed misses on the dev half:

1. Normalise punctuation, backticks and `=` so `sync_capable=True` and
   "sync_capable defaults to True" tokenize alike.
2. Strip stopwords, which carry no evidence and only pad the window.
3. Stem lightly by suffix, so "continues loading" matches "continue loading".
4. Map synonym classes, so "not smaller" matches "not shorter".

Negation parity is enforced, not inherited: a gold phrase that does not negate
cannot be satisfied by a window that does. That is what stops "skips
compression" from matching "does not skip compression". Negations are kept as
tokens rather than dropped as stopwords, because they are the one class of
function word that reverses meaning.

Synonym classes below are the equivalences the gold phrases and the answers
actually disagreed on, not a general thesaurus. Suffix stripping is applied
longest-first so "ies" beats "s", and is deliberately crude: a real
lemmatizer is a dependency and a source of version drift for a graded number.
A bare "continue" must land where "continues" lands: stripping "es" gives
"continu", so the unsuffixed form has to lose its trailing "e" too, or the
two spellings of one word never meet.

window_size bounds how much answer text one gold phrase may be spread across:
wide enough that a phrase restated with extra qualifiers still matches,
narrow enough that unrelated tokens from separate sentences cannot be
stitched into a false positive.
"""

from __future__ import annotations

import re

STOPWORDS = frozenset(
    (
        "a",
        "an",
        "the",
        "is",
        "are",
        "was",
        "were",
        "be",
        "been",
        "being",
        "it",
        "its",
        "this",
        "that",
        "these",
        "those",
        "of",
        "to",
        "in",
        "on",
        "at",
        "by",
        "for",
        "with",
        "from",
        "as",
        "and",
        "or",
        "if",
        "then",
        "than",
        "when",
        "while",
        "into",
        "out",
        "up",
        "down",
        "so",
        "such",
        "each",
        "any",
        "all",
        "both",
        "one",
        "only",
        "other",
        "same",
        "via",
        "per",
        "s",
    )
)

NEGATIONS = frozenset(
    ("not", "no", "never", "neither", "nor", "without", "none", "cannot")
)

SYNONYM_CLASSES: tuple[tuple[str, ...], ...] = (
    ("less", "smaller", "shorter", "fewer", "under", "below", "lower"),
    ("more", "larger", "longer", "greater", "over", "above", "higher"),
    ("omit", "skip", "exclude", "remove", "drop", "bypass", "ignore"),
    ("contain", "include", "match", "have", "carry", "hold"),
    ("raise", "throw"),
    ("call", "invoke", "run", "execute", "apply"),
    ("return", "produce", "yield", "give"),
    ("convert", "transform", "translate", "turn"),
    ("reverse", "backward", "descend"),
    ("begin", "start", "startup", "boot", "load"),
    ("valid", "ok", "success"),
    ("invalid", "bad", "fail", "failure"),
    ("true", "enabled", "on", "set"),
    ("false", "disabled", "off", "unset"),
    ("response", "resp"),
    ("request", "req"),
    ("middleware", "mw"),
)
SYNONYMS = {word: group[0] for group in SYNONYM_CLASSES for word in group}

SUFFIXES = ("ingly", "edly", "ies", "ing", "ers", "est", "ed", "es", "er", "ly", "s")


def stem(token: str) -> str:
    for suffix in SUFFIXES:
        if len(token) > len(suffix) + 2 and token.endswith(suffix):
            base = token[: -len(suffix)]
            return base + "y" if suffix == "ies" else base.rstrip("e")
    return token.rstrip("e") if len(token) > 3 else token


def canonical(token: str) -> str:
    token = SYNONYMS.get(token, token)
    stemmed = stem(token)
    return SYNONYMS.get(stemmed, stemmed)


def tokenize(text: str) -> list[str]:
    lowered = text.lower().replace("_", " ")
    words = re.findall(r"[a-z0-9]+", lowered)
    tokens: list[str] = []
    for word in words:
        if word in NEGATIONS or word.endswith("n't"):
            tokens.append("not")
        elif word not in STOPWORDS:
            tokens.append(canonical(word))
    return tokens


def window_size(phrase_length: int) -> int:
    return max(phrase_length * 3, phrase_length + 10)


def phrase_matches(answer: str, phrase: str) -> bool:
    wanted = tokenize(phrase)
    if not wanted:
        return False
    haystack = tokenize(answer)
    required = set(wanted)
    negated_phrase = "not" in required
    content = required - {"not"}
    if not content:
        return False
    span = window_size(len(wanted))
    for start in range(max(1, len(haystack) - span + 1)):
        window = haystack[start : start + span]
        present = set(window)
        if not content <= present:
            continue
        if negated_phrase != ("not" in present):
            continue
        return True
    return False


def fact_satisfied(answer: str, fact: dict) -> bool:
    return any(phrase_matches(answer, phrase) for phrase in fact["any_of"])
