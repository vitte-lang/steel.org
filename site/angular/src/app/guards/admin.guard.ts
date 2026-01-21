import { inject } from '@angular/core';
import { CanActivateFn, Router } from '@angular/router';

import { ApiService } from '../services/api.service';

export const adminGuard: CanActivateFn = async () => {
  const api = inject(ApiService);
  const router = inject(Router);
  try {
    await api.getStatsAdmin();
    return true;
  } catch {
    return router.parseUrl('/blog');
  }
};
