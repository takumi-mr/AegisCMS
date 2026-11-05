import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { MemberManager } from './MemberManager';

const meta = {
  title: 'Shop/MemberManager',
  component: MemberManager,
  args: { onShowToast: fn() },
} satisfies Meta<typeof MemberManager>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
