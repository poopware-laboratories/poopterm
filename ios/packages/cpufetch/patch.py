"""Teaches cpufetch (in a copy of its sources) the Apple chips its tables
do not know: instead of refusing to run, it shows the chip by the name
apple_chip.c finds, with the core counts sysctl gives and what it cannot
know left unknown. Each replaced text must be found exactly once.

    python3 -I patch.py <copy of cpufetch/src>
"""
import sys
from pathlib import Path

src = Path(sys.argv[1])


def replace(name, old, new):
    path = src / name
    text = path.read_text()
    if text.count(old) != 1:
        sys.exit(f"patch.py: {name}: expected the text once, found it {text.count(old)} times")
    path.write_text(text.replace(old, new))


# An unknown family: two clusters of an unknown core, from sysctl.
replace("arm/midr.c", """  else {
    printBugCheckRelease("Found invalid cpu_family: 0x%.8X", cpu_family);
    return NULL;
  }
""", """  else {
    poop_fill_cpu_info_apple(cpu, pcores, ecores);
    cpu->soc = get_soc(cpu);
    cpu->peak_performance = -1;
  }
""")
replace("arm/midr.c", "struct cpuInfo* get_cpu_info_mach(struct cpuInfo* cpu) {", """struct uarch* poop_unknown_uarch(struct cpuInfo* cpu);

static void poop_fill_cluster(struct cpuInfo* cpu, uint32_t cores) {
  cpu->midr = 0;
  cpu->arch = poop_unknown_uarch(cpu);
  cpu->cach = get_cache_info(cpu);
  cpu->feat = get_features_info();
  cpu->topo = malloc(sizeof(struct topology));
  cpu->topo->cach = cpu->cach;
  cpu->topo->total_cores = cores;
  cpu->freq = malloc(sizeof(struct frequency));
  cpu->freq->base = UNKNOWN_DATA;
  cpu->freq->max = UNKNOWN_DATA;
  cpu->hv = malloc(sizeof(struct hypervisor));
  cpu->hv->present = false;
  cpu->next_cpu = NULL;
}

/* Efficiency cores first, as cpufetch lists the chips it knows. */
static void poop_fill_cpu_info_apple(struct cpuInfo* cpu, uint32_t pcores, uint32_t ecores) {
  poop_fill_cluster(cpu, ecores);
  cpu->next_cpu = malloc(sizeof(struct cpuInfo));
  poop_fill_cluster(cpu->next_cpu, pcores);
}

struct cpuInfo* get_cpu_info_mach(struct cpuInfo* cpu) {""")
# Its uarch, which no MIDR names (an app cannot read the register).
uarch = src / "arm/uarch.c"
uarch.write_text(uarch.read_text() + """
struct uarch* poop_unknown_uarch(struct cpuInfo* cpu) {
  struct uarch* arch = emalloc(sizeof(struct uarch));
  fill_uarch(arch, cpu, "Unknown", UARCH_UNKNOWN, CPU_VENDOR_APPLE);
  return arch;
}
""")
# Its SoC: Apple's, by the name apple_chip.c finds.
replace("arm/soc.c", """  else {
    printBugCheckRelease("Found invalid cpu_family: 0x%.8X", cpu_family);
    soc->soc_vendor = SOC_VENDOR_UNKNOWN;
  }
  return soc;
""", """  else {
    soc->soc_vendor = SOC_VENDOR_APPLE;
    soc->soc_model = SOC_MODEL_UNKNOWN;
    soc->process = UNKNOWN;
    soc->raw_name = poop_apple_chip_name();
  }
  return soc;
""")
soc = src / "arm/soc.c"
soc.write_text("char* poop_apple_chip_name(void);\n" + soc.read_text())
