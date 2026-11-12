import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { mockApi, type MockHandler } from '../../.storybook/mockApi';
import { AcceptInviteForm } from './AcceptInviteForm';

const handler: MockHandler = (req) =>
  req.path === '/auth/invitations/info'
    ? { valid: true, email: 'new.editor@example.com', role: 'editor', display_name: null }
    : undefined;

const meta = {
  beforeEach: () => mockApi(handler),
  title: 'Authentication/AcceptInviteForm',
  component: AcceptInviteForm,
  args: { token: 'story-invite-token', onSuccess: fn(), onBack: fn() },
} satisfies Meta<typeof AcceptInviteForm>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
