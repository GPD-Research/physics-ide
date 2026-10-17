export function claimsFileWriteWithoutOperation(text) {
  const source = String(text || '');
  if (/^\s*write\s+path\s*=\s*["'][^"'\n]+["']\s*\r?\n\s*content\s*=\s*"""[\s\S]*?"""/im.test(source)) {
    return true;
  }

  for (const pathMatch of source.matchAll(/(?:^|[\s"'`])(?:[\w.-]+\/)*[\w.-]+\.(?:md|txt|json|ya?ml|py|rs|js|ts|html|tex|csv|ipynb|docx?)(?=$|[\s"'`.,;:!?])/gi)) {
    const start = Math.max(0, pathMatch.index - 100);
    const end = Math.min(source.length, pathMatch.index + pathMatch[0].length + 100);
    const nearby = source.slice(start, end);
    for (const successMatch of nearby.matchAll(/\b(?:created|written|saved|placed|updated|generated)\b/gi)) {
      const precedingText = nearby.slice(Math.max(0, successMatch.index - 45), successMatch.index);
      if (!/\b(?:not|never|unable|cannot|can't|couldn't|didn't|failed to)\b[^.!?\n]{0,40}$/i.test(precedingText)) {
        return true;
      }
    }
  }
  return false;
}
