import csv
import shutil
import statistics
import numpy as np
import matplotlib.pyplot as plt
from matplotlib.ticker import LogLocator

SUPPORT_TEX = shutil.which("latex") is not None

def set_plot_theme():
    if SUPPORT_TEX:
        plt.rc('text', usetex=True)
        plt.rc('text.latex', preamble=r'\usepackage{mathptmx}')
    else:
        print("WARNING: tex is not installed. Disabling tex labeling in plots.")
    plt.rc('font', family='serif', size=15) # changed from 15
    plt.rc('figure', figsize=(5.5,4))
    COLOR_PALETTE = ['#88CCEE','#CC6677','#DDCC77','#117733', '#332288','#AA4499','#44AA99', '#999933']
    PLT_MARKER = [6, 6, 7, 7]
    WIDE_PLOTS = True

def extrapolate_exponential(data, n_extra=2):
    """
    Extrapolate n_extra points assuming exponential growth.
    Fits log(data) vs index, then predicts further points.
    """
    x = np.arange(len(data))
    y = np.log(data)
    
    # Fit a line in log space
    coeffs = np.polyfit(x, y, 1)
    slope, intercept = coeffs
    
    # Predict next n_extra points
    x_extra = np.arange(len(data), len(data) + n_extra)
    y_extra = slope * x_extra + intercept
    extra_vals = np.exp(y_extra)
    
    return list(data) + list(extra_vals)

XSIZE = 5
YSIZE = 4

set_plot_theme()

x_arr = [pow(2,i) for i in range(13,31)]
data_tiptoe = [0.038, 0.07160, 0.163, 0.306, 1.26, 3.60, 5.43, 9.89, 19.46, 41.67]
data_hintless = [112,156,156,280,324,521,515,1019,1043, 1993, 2004, 3835, 3889]
data_dpf = [0.28, 0.55, 1.10, 2.26, 5.99, 12.45, 24.04, 50.65, 95.77, 208.60, 394.47, 781.04, 1559.40, 3890.53] # in ms
extended_dpf = extrapolate_exponential(data_dpf, n_extra=4)
extended_hintless = extrapolate_exponential(data_hintless, n_extra=6)
ours = {
    "8192":1.04055,
    "16384":1.0241166666666666,
    "32768":1.15035,
    "65536":1.0879666666666667,
    "131072":1.3745666666666667,
    "262144":1.3857,
    "524288":1.7472,
    "1048576":1.6429833333333332,
    "2097152":1.8583166666666666,
    "4194304":2.009183333333333,
    "8388608":2.1737166666666665,
    "16777216":2.5631166666666667,
    "33554432":3.103466666666667,
    "67108864":4.518266666666666,
    "134217728":6.513416666666666,
    "268435456":8.5113,
    "536870912":12.173783333333333,
    "1073741824":24.977816666666666,
}
sorted_keys = sorted(ours.keys(), key=lambda x: int(x))
prc = [ours[k] for k in sorted_keys]

# improvement factor: 67366/24.98 = 2697x faster vs dpf
#                     22272/24.98 = 892x faster vs hintless 



color_dict = {"dpf": "#2066a8",
              "hintless": "#a00000",
              "prc": "#12961f",
            }


plt.figure(figsize=(XSIZE, YSIZE)) # make sure everything renders the same size

lgnds = []

lns = []

l,=plt.plot(x_arr[7:14],extended_dpf[7:14],color=color_dict["dpf"],marker=".",label="Baseline(DPF)")
l,=plt.plot(x_arr[13:],extended_dpf[13:],linestyle='--',color=color_dict["dpf"],marker=".",label="Baseline(DPF)")
lns.append(l)
l,=plt.plot(x_arr[7:12],extended_hintless[8:13],color=color_dict["hintless"],marker=".",label="Baseline(Hintless)")
l,=plt.plot(x_arr[11:],extended_hintless[12:],linestyle='--',color=color_dict["hintless"],marker=".",label="Baseline(Hintless)")
lns.append(l)
l,=plt.plot(x_arr[7:],prc[7:],color=color_dict["prc"],marker=".",label="Our Work")
lns.append(l)

plt.xscale('log',base=2)
plt.yscale('log',base=10)
plt.xlabel('Number of credentials (N)')
# plt.grid(True, which='both', linestyle='-', linewidth=0.5, color='gray', alpha=0.8)
plt.ylabel('Server compute per query (ms)')
plt.ylim(0, 300000)
        
# major ticks every power of 2 (base=2)
plt.gca().xaxis.set_major_locator(LogLocator(base=2, numticks=12))
# minor ticks between powers of 2
# plt.gca().xaxis.set_minor_locator(LogLocator(base=2, subs=np.arange(2, 10) * 0.1, numticks=100))

# major ticks every power of 10 (base=10)
plt.gca().yaxis.set_major_locator(LogLocator(base=10, numticks=10))
# minor ticks between powers of 10
# plt.gca().yaxis.set_minor_locator(LogLocator(base=10, subs=np.arange(2, 10) * 0.1, numticks=100))


lgn_store = plt.legend(handles=lns,loc='upper left', ncol=1,
labelspacing=0.3,
handlelength=1.0,
handletextpad=0.4,
borderpad=0.3,
borderaxespad=0.2)
lgnds.append(lgn_store)
for l in lgnds:
    plt.gca().add_artist(l)

plt.tight_layout()
plt.savefig("naive_pir_plot.pdf", bbox_inches='tight')
plt.show()