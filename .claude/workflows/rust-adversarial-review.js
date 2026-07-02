export const meta = {
  name: 'rust-adversarial-review',
  description: 'Multi-lens adversarial review of the current working-tree diff (unsafe soundness, threading, footprint, platform-API contracts)',
  whenToUse: 'After completing a feature or fix in this repo, before committing: reviews git diff through 4 specialized lenses and adversarially verifies every finding.',
  phases: [
    { title: 'Review', detail: 'four lenses over the working-tree diff' },
    { title: 'Verify', detail: 'adversarial confirmation of each finding' },
  ],
}

const FINDINGS = {
  type: 'object',
  required: ['findings'],
  properties: {
    findings: {
      type: 'array',
      items: {
        type: 'object',
        required: ['file', 'line', 'title', 'detail', 'severity'],
        properties: {
          file: { type: 'string' },
          line: { type: 'number' },
          title: { type: 'string' },
          detail: { type: 'string', description: 'concrete failure scenario: inputs/state -> wrong behavior' },
          severity: { type: 'string', enum: ['critical', 'major', 'minor'] },
        },
      },
    },
  },
}

const VERDICT = {
  type: 'object',
  required: ['isReal', 'reasoning'],
  properties: {
    isReal: { type: 'boolean' },
    reasoning: { type: 'string' },
  },
}

const LENSES = [
  {
    key: 'unsafe-soundness',
    prompt: 'unsafe soundness: missing or wrong // SAFETY: comments, potential UB, dangling handles, missing Drop/RAII cleanup for hooks and windows, unsafe outside platform adapter crates (forbidden by CLAUDE.md)',
  },
  {
    key: 'threading',
    prompt: 'threading and event-loop correctness: blocked message pumps, COM apartment violations (TSF/UIA must be STA), CFRunLoop requirements, channel deadlocks, data shared across threads without Send/Sync justification, callbacks crossing thread boundaries',
  },
  {
    key: 'footprint',
    prompt: 'footprint regressions: polling loops or timers that violate the zero-polling rule (ADR-0003), timers that never disarm when the overlay hides, allocations or repaints in per-mouse-move hot paths, resources not released when idle',
  },
  {
    key: 'platform-contract',
    prompt: 'platform API contract misuse: Win32/AppKit/X11/D-Bus flags or return codes used contrary to documentation, ignored error codes, missing WM_DPICHANGED / mixed-DPI handling, wrong window ex-styles for click-through overlay (see ADR-0004), APIs called from the wrong thread',
  },
]

phase('Review')
const results = await pipeline(
  LENSES,
  l =>
    agent(
      `You are reviewing the current uncommitted changes of this Rust repo (lang-switcher). Run "git diff HEAD" (plus "git status --short" for untracked files, read them too). Read enough surrounding code to judge in context. Report ONLY real defects visible in or caused by the changed code — no style nits. Lens: ${l.prompt}. Repo rules live in CLAUDE.md and docs/architecture/. Return structured findings; empty list if nothing real.`,
      { label: `review:${l.key}`, phase: 'Review', schema: FINDINGS },
    ),
  (review, lens) =>
    review && review.findings.length
      ? parallel(
          review.findings.map(f => () =>
            agent(
              `Adversarially verify this code-review finding in repo lang-switcher. Try to REFUTE it by reading the actual code at ${f.file}:${f.line} and its callers. Finding (lens ${lens.key}): ${f.title} — ${f.detail}. Default to isReal=false unless the failure scenario demonstrably holds.`,
              { label: `verify:${lens.key}`, phase: 'Verify', schema: VERDICT },
            ).then(v => (v && v.isReal ? { ...f, lens: lens.key, verdict: v.reasoning } : null)),
          ),
        )
      : [],
)

const confirmed = results
  .filter(Boolean)
  .flat()
  .filter(Boolean)
  .sort((a, b) => ['critical', 'major', 'minor'].indexOf(a.severity) - ['critical', 'major', 'minor'].indexOf(b.severity))

log(`Confirmed findings: ${confirmed.length}`)
return { confirmed }
