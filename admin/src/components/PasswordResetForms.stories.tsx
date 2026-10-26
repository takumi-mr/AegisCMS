import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { PasswordResetConfirmForm, PasswordResetRequestForm } from './PasswordResetForms';

const meta = {
  title: 'Authentication/PasswordResetForms',
  component: PasswordResetRequestForm,
  args: { onBack: fn() },
} satisfies Meta<typeof PasswordResetRequestForm>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Request: Story = {};
export const Confirm: Story = {
  render: () => (
    <PasswordResetConfirmForm token="story-reset-token" onBack={fn()} onSuccess={fn()} />
  ),
};
