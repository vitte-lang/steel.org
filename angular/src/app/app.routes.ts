import { Routes } from '@angular/router';

export const routes: Routes = [
  {
    path: '',
    loadComponent: () => import('./home.component').then((m) => m.HomeComponent)
  },
  {
    path: 'downloads',
    loadComponent: () => import('./downloads.component').then((m) => m.DownloadsComponent)
  },
  {
    path: 'docs',
    loadComponent: () => import('./docs.component').then((m) => m.DocsComponent)
  },
  {
    path: 'showcase',
    loadComponent: () => import('./showcase.component').then((m) => m.ShowcaseComponent)
  },
  {
    path: 'security',
    loadComponent: () => import('./security.component').then((m) => m.SecurityComponent)
  },
  {
    path: 'status',
    loadComponent: () => import('./status.component').then((m) => m.StatusComponent)
  },
  { path: '**', redirectTo: '' }
];
