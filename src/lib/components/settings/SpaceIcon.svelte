<script lang="ts">
  import {
    BookOpen,
    BrainCircuit,
    Briefcase,
    Code2,
    Folder,
    Heart,
    Inbox,
    Layers,
    Plane,
    Star,
    UserRound,
    Users
  } from "lucide-svelte";

  const icons = {
    layers: Layers,
    user: UserRound,
    briefcase: Briefcase,
    folder: Folder,
    inbox: Inbox,
    users: Users,
    star: Star,
    sparkles: BrainCircuit,
    heart: Heart,
    plane: Plane,
    book: BookOpen,
    code: Code2
  } as const;

  let {
    name = "layers",
    size = 15
  }: {
    name?: string;
    size?: number;
  } = $props();

  const Icon = $derived(icons[name as keyof typeof icons] ?? Layers);
</script>

<Icon {size} />
