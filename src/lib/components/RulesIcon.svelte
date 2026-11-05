<script lang="ts">
  let { size = 15 }: { size?: number } = $props();
</script>

<!-- Two arrows, one straight up and one branching up to the right: a mail
     being routed by a rule. Drawn to match the icon used in the reference. -->
<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d="M9 20V6" />
  <path d="M6 9 9 6l3 3" />
  <path d="M9 16.5c5 0 9-3.5 9-10.5" />
  <path d="M15 9l3-3 3 3" />
</svg>
