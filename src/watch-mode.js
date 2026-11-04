// Watch-mode presentation helpers (pure, unit-tested). The Rust side owns baselines and comparisons.

export function asciiMeter(fraction, width = 20) {
  const f = Math.max(0, Math.min(1, Number(fraction) || 0));
  const filled = Math.round(f * width);
  return `[${'#'.repeat(filled)}${'-'.repeat(width - filled)}]`;
}

export function runMeter(elapsedSeconds, limitSeconds) {
  const limit = Math.max(1, Number(limitSeconds) || 60);
  const elapsed = Math.max(0, Math.floor(Number(elapsedSeconds) || 0));
  return `${asciiMeter(elapsed / limit)} ${elapsed}s / ${limit}s limit`;
}

const fmtNum = (v) => (v == null || !Number.isFinite(Number(v)) ? '—' : Number(Number(v).toPrecision(6)).toString());

function fmtResult(r) {
  if (!r) return '—';
  const err = r.err == null ? '' : `±${fmtNum(r.err)}`;
  return `${fmtNum(r.value)}${err}${r.unit ? ` ${r.unit}` : ''}`;
}

export function describeEvery(seconds) {
  const s = Number(seconds) || 0;
  if (s <= 0) return 'on input change';
  if (s % 3600 === 0) return `every ${s / 3600} h`;
  if (s % 60 === 0) return `every ${s / 60} min`;
  return `every ${s} s`;
}

// Text for a reportable run: shown in the thread and sent to the AI as the Watch message.
export function formatWatchReport(report, watch) {
  const op = report.op_id ? ` (op#${report.op_id})` : '';
  const lines = [`**Watch #${watch.id} — ${report.status}**${op}: \`${watch.cmd}\``];
  if (report.changed_inputs?.length) lines.push(`Changed inputs: ${report.changed_inputs.join(', ')}`);
  if (report.status === 'failed') {
    lines.push(`The run failed: ${report.note || 'no detail'}. The baseline is unchanged.`);
    lines.push(`Diagnose the failure from the op#${report.op_id || '?'} output; do not report values from this run.`);
    return lines.join('\n');
  }
  if (report.note && !report.changes?.length) lines.push(report.note);
  const notable = (report.changes || []).filter((c) => c.kind !== 'steady');
  if (notable.length) {
    lines.push('', '| RESULT | change | baseline | now | Δ | z |', '|---|---|---|---|---|---|');
    notable.forEach((c) => {
      const delta = c.old && c.new ? fmtNum(c.new.value - c.old.value) : '—';
      const z = c.z == null ? (c.note || '—') : `${c.z.toFixed(1)}σ`;
      lines.push(`| ${c.name} | ${c.kind.replace('_', ' ')} | ${fmtResult(c.old)} | ${fmtResult(c.new)} | ${delta} | ${z} |`);
    });
    const steady = (report.changes || []).length - notable.length;
    lines.push('', `${steady} other RESULT(s) within ${report.sigma}σ of the baseline.`);
    lines.push(`Interpret this change: which changed input plausibly moved which prediction, and is it physical or a bug? Cite op#${report.op_id || '?'}; do not claim the new value is correct without a check.`);
  }
  return lines.join('\n');
}
