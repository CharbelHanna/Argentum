// Each installer a release attaches, against the same file of the release before it. Ours.
//
// North star 7: installer size is the smoke alarm, and a release that grows it noticeably says
// why. This is the alarm. It fails when any installer grew by more than GROWTH_LIMIT, unless the
// release's notes carry a line starting `Installer size:` that explains it; then it passes and
// still prints the table. release.yml runs it once every installer is attached. To clear a
// failure, add the line to the notes and re-run the job.
//
// Run:  node scripts/installer-size.mjs <tag>   (needs gh, GH_REPO or a checkout)
// Test: node scripts/test-installer-size.mjs
import { execFileSync } from 'node:child_process';
import { appendFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

/** Growth per installer, as a fraction, that needs an explanation in the notes. */
export const GROWTH_LIMIT = 0.03;

/** The files a person installs; the `.app.tar.gz` updater archives are not. */
const INSTALLER = /\.(exe|msi|dmg|deb|rpm|AppImage)$/;

/** A file's name with its version taken out, the same for every release. */
export const sizeKey = (name) => name.replace(/_v\d+\.\d+\.\d+_/, '_v_');

/** Whether the notes explain a growth: a line starting `Installer size:`, bold or not. */
export const explained = (notes) => /^\W*installer size\W*:/im.test(notes ?? '');

/**
 * One row per installer of `current` that `previous` also has, largest growth first, and the
 * failures: rows over the limit when the notes do not explain them. Assets are [{ name, size }].
 */
export function sizeReport(previous, current, notes) {
  const before = new Map(previous.map((a) => [sizeKey(a.name), a.size]));
  const rows = current
    .filter((a) => INSTALLER.test(a.name) && before.has(sizeKey(a.name)))
    .map((a) => {
      const was = before.get(sizeKey(a.name));
      return { name: a.name, was, is: a.size, growth: a.size / was - 1 };
    })
    .sort((a, b) => b.growth - a.growth);
  const over = rows.filter((row) => row.growth > GROWTH_LIMIT);
  return { rows, failures: explained(notes) ? [] : over };
}

const mb = (bytes) => (bytes / 1048576).toFixed(1);
const pct = (fraction) => `${fraction >= 0 ? '+' : ''}${(fraction * 100).toFixed(1)}%`;

/** The report as a Markdown table. */
export function table(previousTag, rows) {
  return [
    `| Installer | ${previousTag} MB | MB | Change |`,
    '|---|---:|---:|---:|',
    ...rows.map((r) => `| ${r.name} | ${mb(r.was)} | ${mb(r.is)} | ${pct(r.growth)} |`),
  ].join('\n');
}

function gh(args) {
  return JSON.parse(execFileSync('gh', args, { encoding: 'utf8' }));
}

function main(tag) {
  if (!tag) throw new Error('usage: node scripts/installer-size.mjs <tag>');
  const releases = gh(['release', 'list', '--exclude-drafts', '--limit', '50', '--json', 'tagName,publishedAt']);
  const current = gh(['release', 'view', tag, '--json', 'assets,body,publishedAt']);
  const previousTag = releases
    .filter((r) => r.tagName !== tag && r.publishedAt < current.publishedAt)
    .sort((a, b) => b.publishedAt.localeCompare(a.publishedAt))[0]?.tagName;
  if (!previousTag) {
    console.log(`  ${tag}: no earlier release to compare with`);
    return;
  }
  const previous = gh(['release', 'view', previousTag, '--json', 'assets']);
  const { rows, failures } = sizeReport(previous.assets, current.assets, current.body);
  const report = `## Installer size, ${tag} against ${previousTag}\n\n${table(previousTag, rows)}\n`;
  console.log(report);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, report);
  if (rows.length === 0) throw new Error(`${tag}: no installer matches one of ${previousTag}'s`);
  if (failures.length > 0) {
    throw new Error(
      `${failures.length} installer(s) grew by more than ${GROWTH_LIMIT * 100}%: ` +
        `${failures.map((r) => `${r.name} ${pct(r.growth)}`).join(', ')}. ` +
        'Say why in the notes (a line starting "Installer size:") and re-run this job.',
    );
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  try {
    main(process.argv[2]);
  } catch (error) {
    console.error(`  ${error.message}`);
    process.exitCode = 1;
  }
}
