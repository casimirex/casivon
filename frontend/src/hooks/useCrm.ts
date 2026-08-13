import { useQuery } from '@tanstack/react-query';
import { http } from '@/api/client';
import { createResource } from './useResource';
import type { Activity, Company, Contact, Opportunity, PipelineStage } from '@/types';

export const contacts = createResource<Contact>('/crm/contacts', 'contacts');
export const companies = createResource<Company>('/crm/companies', 'companies');
export const opportunities = createResource<Opportunity>('/crm/opportunities', 'opportunities');
export const activities = createResource<Activity>('/crm/activities', 'activities');

/** Open pipeline value by stage, for the CRM dashboard. */
export function usePipeline() {
  return useQuery({
    queryKey: ['opportunities', 'pipeline'],
    queryFn: () => http.get<PipelineStage[]>('/crm/opportunities/pipeline'),
  });
}

/**
 * Companies for a `<Select>`. Loads one large page rather than paging, since a
 * picker needs the whole list; swap for a typeahead if the org outgrows it.
 */
export function useCompanyOptions() {
  const { data, isLoading } = companies.useList({ per_page: 200, sort: 'name' });
  return {
    isLoading,
    options: (data?.data ?? []).map((company) => ({
      value: company.id,
      label: company.name,
    })),
  };
}

export function useContactOptions(companyId?: string) {
  const { data, isLoading } = contacts.useList({
    per_page: 200,
    company_id: companyId,
  });
  return {
    isLoading,
    options: (data?.data ?? []).map((contact) => ({
      value: contact.id,
      label: `${contact.first_name} ${contact.last_name}`,
    })),
  };
}
