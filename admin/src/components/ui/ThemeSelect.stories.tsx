import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, userEvent, within } from 'storybook/test';
import { THEME_STORAGE_KEY } from '../../theme';
import { ThemeSelect } from './ThemeSelect';

const meta = { title: 'UI/ThemeSelect', component: ThemeSelect } satisfies Meta<typeof ThemeSelect>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  play: async ({ canvasElement }) => {
    const select = within(canvasElement).getByRole('combobox', { name: /theme|テーマ/i });

    // Light and dark apply right away and are remembered in this browser
    await userEvent.selectOptions(select, 'light');
    await expect(document.documentElement.dataset.theme).toBe('light');
    await expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe('light');

    await userEvent.selectOptions(select, 'dark');
    await expect(document.documentElement.dataset.theme).toBe('dark');
    await expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe('dark');

    // "System" follows the OS (light or dark, depending on the browser running the test)
    await userEvent.selectOptions(select, 'system');
    await expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe('system');
    await expect(['light', 'dark']).toContain(document.documentElement.dataset.theme);
    await expect(select).toHaveValue('system');
  },
};
