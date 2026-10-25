import { describe, it, expect } from 'vitest';
import type { NotificationKindInfo, NotificationTemplateDetail } from '../types';
import {
  editsStaff,
  isDirty,
  isSharedPart,
  previewTarget,
  templateBase,
  templateNames,
  unsavedTemplates,
  type EditorState,
} from './notificationTemplates';

const kind = (person: boolean, staff: boolean): NotificationKindInfo => ({
  kind: 'x',
  person,
  staff,
  disabled: false,
});

const editor = (name: string, saved: string | null, draft: string): EditorState => {
  const detail: NotificationTemplateDetail = {
    name,
    current: saved === null ? null : { source: 'builtin', content: saved },
    default: null,
  };
  return { detail, draft };
};

describe('templateBase / isSharedPart', () => {
  it('maps every audit event to the shared audit templates', () => {
    expect(templateBase('audit.session.create')).toBe('audit');
    expect(templateBase('order.placed')).toBe('order.placed');
  });

  it('tells shared parts from notification kinds', () => {
    expect(isSharedPart('_layout.html')).toBe(true);
    expect(isSharedPart('order.placed')).toBe(false);
  });
});

describe('editsStaff', () => {
  it('follows the toggle when the kind has both audiences', () => {
    expect(editsStaff(kind(true, true), false)).toBe(false);
    expect(editsStaff(kind(true, true), true)).toBe(true);
    expect(editsStaff(undefined, true)).toBe(true);
  });

  it('forces the only audience a kind has', () => {
    expect(editsStaff(kind(false, true), false)).toBe(true);
    expect(editsStaff(kind(true, false), true)).toBe(false);
  });
});

describe('templateNames', () => {
  it('lists the subject, text and HTML of a kind', () => {
    expect(templateNames('order.placed', false, true)).toEqual([
      'order.placed.subject',
      'order.placed.txt',
      'order.placed.html',
    ]);
  });

  it('uses the .staff templates and leaves HTML out when it is off', () => {
    expect(templateNames('order.placed', true, false)).toEqual([
      'order.placed.staff.subject',
      'order.placed.staff.txt',
    ]);
    expect(templateNames('audit.session.create', true, true)[0]).toBe('audit.staff.subject');
  });

  it('edits a shared part on its own', () => {
    expect(templateNames('_footer.txt', true, true)).toEqual(['_footer.txt']);
  });
});

describe('previewTarget', () => {
  it('previews a kind for the chosen audience', () => {
    expect(previewTarget('comment.pending', true)).toEqual({
      kind: 'comment.pending',
      staff: true,
    });
  });

  it('previews a shared part with a notification that uses it', () => {
    expect(previewTarget('_summary.txt', true)).toEqual({ kind: 'order.placed', staff: false });
    expect(previewTarget('_audit_title.txt', false).kind).toBe('audit.session.create');
    expect(previewTarget('_layout.html', false).kind).toBe('test');
  });
});

describe('isDirty / unsavedTemplates', () => {
  it('compares the draft with the saved template', () => {
    expect(isDirty(editor('a.txt', 'Hello', 'Hello'))).toBe(false);
    expect(isDirty(editor('a.txt', 'Hello', 'Hi'))).toBe(true);
    // A template that does not exist yet is not dirty until something is written
    expect(isDirty(editor('a.html', null, ''))).toBe(false);
    expect(isDirty(editor('a.html', null, '<p>'))).toBe(true);
  });

  it('sends only non-empty edits to the preview', () => {
    expect(
      unsavedTemplates({
        'a.subject': editor('a.subject', 'Subject', 'Subject'),
        'a.txt': editor('a.txt', 'Body', 'New body'),
        'a.html': editor('a.html', '<p>Body</p>', ''),
      })
    ).toEqual({ 'a.txt': 'New body' });
  });
});
