import { useTranslation } from 'react-i18next';

const ROLES = ['admin', 'editor', 'viewer'] as const;
type Role = (typeof ROLES)[number];
const isRole = (role: string): role is Role => (ROLES as readonly string[]).includes(role);

/** The role's display name in the current language (unknown roles as they are) */
export const useRoleName = () => {
  const { t } = useTranslation();
  return (role: string): string => (isRole(role) ? t(`common.roles.${role}`) : role);
};
