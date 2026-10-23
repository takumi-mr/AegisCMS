import React from 'react';
import { useRoleName } from '../../i18n/roles';

/** A role's display name in the current language */
export const RoleLabel: React.FC<{ role: string }> = ({ role }) => <>{useRoleName()(role)}</>;
