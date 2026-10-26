import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { ScheduleCalendar } from './ScheduleCalendar';

const meta = {
  title: 'Content/ScheduleCalendar',
  component: ScheduleCalendar,
  args: { onOpenContent: fn(), onShowToast: fn() },
} satisfies Meta<typeof ScheduleCalendar>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Calendar: Story = {};
