import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { AcceptInviteForm } from './AcceptInviteForm';

const meta = {
  title: 'Authentication/AcceptInviteForm',
  component: AcceptInviteForm,
  args: { token: 'story-invite-token', onSuccess: fn(), onBack: fn() },
} satisfies Meta<typeof AcceptInviteForm>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
