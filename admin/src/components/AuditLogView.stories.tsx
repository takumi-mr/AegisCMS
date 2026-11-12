import type { Meta, StoryObj } from '@storybook/react-vite';
import { mockApi, type MockHandler } from '../../.storybook/mockApi';
import type { AuditAction, AuditEvent, AuditLogPage, AuditOutcome } from '../types';
import { AuditLogView } from './AuditLogView';

const event = (
  id: string,
  action: AuditAction,
  outcome: AuditOutcome,
  actor: string | null,
  at: string,
  details: Record<string, unknown> = {}
): AuditEvent => ({
  id,
  occurred_at: at,
  action,
  outcome,
  actor: actor ? { user_id: actor, role: 'admin' } : null,
  resource_type: action.split('.')[0],
  resource_id: id,
  ip: '203.0.113.7',
  user_agent: 'Mozilla/5.0',
  details,
});

const page: AuditLogPage = {
  items: [
    event('e1', 'session.create', 'success', 'ada', '2026-11-12T09:00:00Z', { provider: 'local' }),
    event('e2', 'content.publish', 'success', 'ada', '2026-11-12T09:05:00Z'),
    event('e3', 'session.create', 'failure', null, '2026-11-12T09:10:00Z', {
      username: 'admin',
      error: 'invalid_password',
    }),
    event('e4', 'access.denied', 'denied', 'bob', '2026-11-12T09:12:00Z', {
      reason: 'insufficient_role',
    }),
  ],
  limit: 50,
  offset: 0,
  retention_days: 90,
};

const handler: MockHandler = (req) => (req.path === '/audit-logs' ? page : undefined);

const meta = {
  beforeEach: () => mockApi(handler),
  title: 'Settings/AuditLogView',
  component: AuditLogView,
} satisfies Meta<typeof AuditLogView>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
