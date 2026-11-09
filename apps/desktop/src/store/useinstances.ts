import { useCallback, useState } from "react";
import type { CloneInstanceInput, CreateInstanceInput, Instance, UpdateInstanceInput } from "../../shared/types/instance";
import { launcher } from "../bridge";
import { formatError } from "./errors";
import { removeInstance as dropInstance, resolveFallbackActiveId, sortInstances, upsertInstance } from "./instancestate";

interface UseInstancesOptions {
  onActiveChange?: (instance: Instance) => void;
}

export function useInstances({ onActiveChange }: UseInstancesOptions = {}) {
  const [instances, setInstances] = useState<Instance[]>([]);
  const [activeInstance, setActiveInstanceState] = useState<Instance | null>(null);
  const [instancesError, setInstancesError] = useState<string | null>(null);

  const applyInstance = useCallback((instance: Instance) => {
    setInstances((current) => sortInstances(upsertInstance(current, instance)));
    setActiveInstanceState((current) => (current?.id === instance.id ? instance : current));
  }, []);

  const loadInstances = useCallback(async () => {
    setInstancesError(null);

    const [listed, active] = await Promise.allSettled([launcher.instances.list(), launcher.instances.getActive()]);
    const failure = [listed, active].find((result): result is PromiseRejectedResult => result.status === "rejected");

    setInstances(listed.status === "fulfilled" ? sortInstances(listed.value) : []);
    setActiveInstanceState(active.status === "fulfilled" ? active.value : null);
    setInstancesError(failure ? formatError(failure.reason) : null);
  }, []);

  const refreshInstance = useCallback(
    async (id: string) => {
      applyInstance(await launcher.instances.get(id));
    },
    [applyInstance]
  );

  const setActiveInstance = useCallback(
    async (id: string) => {
      const instance = await launcher.instances.setActive(id);
      setActiveInstanceState(instance);
      applyInstance(instance);
      onActiveChange?.(instance);
      return instance;
    },
    [applyInstance, onActiveChange]
  );

  const createInstance = useCallback(
    async (input: CreateInstanceInput) => {
      const instance = await launcher.instances.create(input);
      applyInstance(instance);
      return instance;
    },
    [applyInstance]
  );

  const updateInstance = useCallback(
    async (id: string, patch: UpdateInstanceInput) => {
      const instance = await launcher.instances.update(id, patch);
      applyInstance(instance);
      return instance;
    },
    [applyInstance]
  );

  const cloneInstance = useCallback(
    async (input: CloneInstanceInput) => {
      const instance = await launcher.instances.clone(input);
      applyInstance(instance);
      return instance;
    },
    [applyInstance]
  );

  const removeInstance = useCallback(
    async (id: string) => {
      if (activeInstance?.id === id) {
        const fallbackId = resolveFallbackActiveId(instances, id);

        if (!fallbackId) {
          throw new Error("The last remaining instance cannot be removed.");
        }

        await setActiveInstance(fallbackId);
      }

      await launcher.instances.remove(id);
      setInstances((current) => dropInstance(current, id));
    },
    [activeInstance?.id, instances, setActiveInstance]
  );

  return {
    instances,
    activeInstance,
    instancesError,
    loadInstances,
    refreshInstance,
    setActiveInstance,
    createInstance,
    updateInstance,
    cloneInstance,
    removeInstance
  };
}
