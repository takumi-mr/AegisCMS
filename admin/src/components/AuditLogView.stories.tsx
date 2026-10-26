import type { Meta, StoryObj } from '@storybook/react-vite';
import { AuditLogView } from './AuditLogView';

const meta = { title: 'Settings/AuditLogView', component: AuditLogView } satisfies Meta<
  typeof AuditLogView
>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
