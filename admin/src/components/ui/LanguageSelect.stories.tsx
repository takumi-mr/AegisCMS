import type { Meta, StoryObj } from '@storybook/react-vite';
import { LanguageSelect } from './LanguageSelect';

const meta = { title: 'UI/LanguageSelect', component: LanguageSelect } satisfies Meta<
  typeof LanguageSelect
>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
