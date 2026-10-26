import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { LoginView, SessionCheckingScreen } from './LoginView';

const meta = {
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
