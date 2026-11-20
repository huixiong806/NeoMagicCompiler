// =========================================================================
//  Parser — separates JSON config header from code body in ```riscv blocks
// =========================================================================

import type { EditorConfig, RiscvBlock } from './types';

/**
 * Normalize line endings (CRLF / CR → LF).
 */
export function normalizeNewlines(text: string): string {
  return (text || '').replace(/\r\n/g, '\n').replace(/\r/g, '\n');
}

/**
 * Parse a ` ```riscv ` fenced block into config + code.
 *
 * Supported formats:
 *   { JSON config }
 *   ---
 *   multi-line code…
 *
 * Fallback (backward-compatible): JSON without `---`, separated at last `}`.
 * Pure code blocks (no JSON preamble) return `config: null`.
 */
export function parseRiscvBlock(rawText: string): RiscvBlock {
  const text = normalizeNewlines(rawText).trim();
  const sepIdx = text.indexOf('\n---\n');

  let jsonStr: string;
  let codeStr: string;

  if (sepIdx !== -1) {
    jsonStr = text.substring(0, sepIdx).trim();
    codeStr = text.substring(sepIdx + 5).trim();
  } else if (text[0] === '{') {
    const lb = text.lastIndexOf('}');
    if (lb === -1) {
      return { config: null, code: text, error: null };
    }
    jsonStr = text.substring(0, lb + 1).trim();
    codeStr = text.substring(lb + 1).trim();
  } else {
    return { config: null, code: text, error: null };
  }

  try {
    const config: EditorConfig = JSON.parse(jsonStr);
    return { config, code: codeStr, error: null };
  } catch (e) {
    return {
      config: null,
      code: '',
      error: `关卡配置 JSON 解析失败：${(e as Error).message}`,
    };
  }
}

/**
 * Build a localStorage key scoped to a level.
 */
export function storageKey(levelId: string): string {
  return `level-${levelId}-draft`;
}
