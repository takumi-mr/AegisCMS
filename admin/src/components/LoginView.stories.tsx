import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { mockApi, type MockHandler } from '../../.storybook/mockApi';
import { LoginView, SessionCheckingScreen } from './LoginView';

const handler: MockHandler = (req) => {
  if (req.path === '/setup/status') return { initialized: true, requires_setup: false };
  if (req.path === '/auth/providers') {
    return { local: true, oidc: null, saml: null, registration: true, password_reset: true };
  }
  return undefined;
};

const meta = {
  beforeEach: () => mockApi(handler),
  title: 'Authentication/LoginView',
  component: LoginView,
  args: { onLoggedIn: fn() },
} satisfies Meta<typeof LoginView>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
export const WithNotice: Story = {
  args: { notice: 'Your session has expired. Please sign in again.' },
};
export const SessionChecking: Story = { render: () => <SessionCheckingScreen /> };
