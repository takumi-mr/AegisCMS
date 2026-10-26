import type { Meta, StoryObj } from '@storybook/react-vite';
import { RoleLabel } from './RoleLabel';

const meta = {
  title: 'UI/RoleLabel',
  component: RoleLabel,
  args: { role: 'editor' },
} satisfies Meta<typeof RoleLabel>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Admin: Story = { args: { role: 'admin' } };
export const Editor: Story = { args: { role: 'editor' } };
export const Viewer: Story = { args: { role: 'viewer' } };
