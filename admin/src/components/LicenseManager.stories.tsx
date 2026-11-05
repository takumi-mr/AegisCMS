import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, userEvent, within } from 'storybook/test';
import { LicenseManager } from './LicenseManager';

const meta = {
  title: 'Settings/LicenseManager',
  component: LicenseManager,
} satisfies Meta<typeof LicenseManager>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    // AegisCMS core license heading should be visible (matches either en or ja)
    await expect(canvas.getByText(/AegisCMS (License|ライセンス)/)).toBeVisible();
    await expect(canvas.getByText('MIT License')).toBeVisible();

    // Default shows search input
    const searchInput = canvas.getByRole('searchbox');
    await expect(searchInput).toBeVisible();

    // Test filter tabs
    const tabs = canvas.getAllByRole('tab');
    await expect(tabs.length).toBe(3);

    // Switch to Rust tab
    await userEvent.click(tabs[1]);
    await expect(tabs[1]).toHaveAttribute('aria-selected', 'true');

    // Switch to npm tab
    await userEvent.click(tabs[2]);
    await expect(tabs[2]).toHaveAttribute('aria-selected', 'true');

    // Search filtering
    await userEvent.type(searchInput, 'react');
    await expect(canvas.getByText('react')).toBeVisible();
  },
};
