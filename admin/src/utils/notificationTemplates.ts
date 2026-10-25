import type { NotificationKindInfo, NotificationTemplateDetail } from '../types';

/** Templates for a notification kind live under `<base>` (`audit.*` share the `audit` templates) */
export const templateBase = (kind: string): string => (kind.startsWith('audit.') ? 'audit' : kind);

/** A shared part (`_footer.txt`, `_layout.html`…) rather than a notification kind */
export const isSharedPart = (target: string): boolean => target.startsWith('_');

/** The notification a shared part is previewed with */
const PART_PREVIEW_KIND: Record<string, string> = {
  '_summary.txt': 'order.placed',
  '_comment_summary.txt': 'comment.pending',
  '_audit_title.txt': 'audit.session.create',
};

/**
 * Whether the staff templates are edited: kinds with templates for one audience only
 * (`account.registered` is staff only) always use that audience, others follow the toggle
 */
export const editsStaff = (kind: NotificationKindInfo | undefined, wantStaff: boolean): boolean => {
  if (kind && !kind.person) return true;
  if (kind && !kind.staff) return false;
  return wantStaff;
};

/** Template names edited for a target: the part itself, or the kind's subject / text (/ HTML) */
export const templateNames = (target: string, staff: boolean, html: boolean): string[] => {
  if (isSharedPart(target)) return [target];
  const base = templateBase(target) + (staff ? '.staff' : '');
  return ['subject', 'txt', ...(html ? ['html'] : [])].map((ext) => `${base}.${ext}`);
};

/** The notification (and audience) rendered in the preview for a target */
export const previewTarget = (target: string, staff: boolean): { kind: string; staff: boolean } =>
  isSharedPart(target)
    ? { kind: PART_PREVIEW_KIND[target] ?? 'test', staff: false }
    : { kind: target, staff };

/** A template in the editor */
export interface EditorState {
  detail: NotificationTemplateDetail;
  /** Text in the editor */
  draft: string;
}

export const isDirty = (state: EditorState): boolean =>
  state.draft !== (state.detail.current?.content ?? '');

/** Unsaved, non-empty edits (name → text) for the preview */
export const unsavedTemplates = (editors: Record<string, EditorState>): Record<string, string> =>
  Object.fromEntries(
    Object.values(editors)
      .filter((state) => isDirty(state) && state.draft !== '')
      .map((state) => [state.detail.name, state.draft])
  );
