import csv
import shutil
import statistics
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

XSIZE = 5
YSIZE = 4

def parse_file(name):
    with open(name, 'r') as file:
        reader = csv.reader(file)
        next(reader)

        # Read the data rows
        data= []
        for row in reader:
            data.append(row)
        return data
    
# returns a list with all numbers of records considered
def get_num_records(data):
    results = []
    for row in data:
        try:
            l1 = int(row[2].strip())
            l2 = int(row[3].strip())
            prod = l1*l2
            if prod not in results:
                results.append(l1 * l2)
        except (ValueError, IndexError):
            # Skip rows with missing or non-integer values
            continue

    return results

def string_to_ms(s):
    try: # first try this, because comparing greek characters is pain
        return float(s[:-1]) * 1000
    except:
        if s.endswith('ms'):
            return float(s[:-2])
        else:
            return float(s[:-2]) / 1000

def get_val_dict(data,idx):
    val_sum_per_num_servers = {}
    result = {} # average of the above
    for row in data: 
        num_servers = int(row[1].strip())
        record_num = int(row[2])*int(row[3])

        if idx==4:
            val = string_to_ms(row[4].strip())
        elif idx ==5:
            val = string_to_ms(row[5].strip())
        elif idx ==6: # communication cost -> only give upload as upload=download
            val = (float(row[6].strip()))/1024
            # val = (float(row[6].strip())+float(row[7].strip()))/1024
        elif idx==42: # them moneeeee
            uploadKiB = (float(row[7].strip()))/1024 # download
            uploadMiB = uploadKiB/1024 # download
            uploadgB = uploadMiB/1024 # download
            comphrs = string_to_ms(row[5].strip())/3600000

            val = uploadgB*9+comphrs*4
            val = val * 10000 # one million queries in dollar
        else: 
            val = 0

        server_dict = val_sum_per_num_servers.get(num_servers,{})
        server_dict[record_num]=server_dict.get(record_num,[])+[val]
        val_sum_per_num_servers[num_servers]=server_dict

    for (k,d) in val_sum_per_num_servers.items():
        if(k==5 or k==3): # skip 3 and 5 servers
            continue
        intermediate = {}
        for (k2,v) in d.items():
            intermediate[k2]=statistics.mean(v)
        result[k]=intermediate 

    return result 

set_plot_theme()

data = []

for name in ["measure_delay_tcp_rtt_20ms.csv","measure_delay_tcp_rtt_40ms.csv", "measure_delay_tcp_rtt_80ms.csv"]:
    data.append(parse_file(name))

x = get_num_records(data[0])
x.sort()

color_dict = {"2"+"20": "#8cc5e3",
              "4"+"20": "#3594cc",
              "8"+"20": "#2066a8",
              "2"+"40": "#d8a6a6",
              "4"+"40": "#c46666",
              "8"+"40": "#a00000",
              "2"+"80": "#9fc8c8",
              "4"+"80": "#54a1a1",
              "8"+"80": "#1f6f6f",
            }

color_dict2= {"2": "#8cc5e3",
              "4": "#3594cc",
              "8": "#2066a8",
            }

color_dict3= {"2": "#d8a6a6",
              "4": "#c46666",
              "8": "#a00000",
            }

color_dict4= {"2": "#8cc5e3",
              "4": "#3594cc",
              "8": "#2066a8",
            }


plt.figure(figsize=(XSIZE, YSIZE)) # make sure everything renders the same size

# plot data y: latency, x: records for each combination of number servers and network delay
lat = [get_val_dict(data[i],4) for i in [0,1,2]]

lgnds = []

for i in range(len(lat)):
    lns = []
    for (servers,plot_data) in lat[i].items():
        y = [plot_data[rec] for rec in x]

        l,=plt.plot(x,y,color=color_dict[str(servers)+str(pow(2,i)*20)],marker=".",label="n="+str(servers))
        lns.append(l)
        plt.xscale('log',base=2)
        plt.gca().xaxis.set_major_locator(LogLocator(base=2, numticks=10))
        plt.xlabel('Number of records (N)')
        # plt.grid(True, which='both', linestyle='-', linewidth=0.5, color='gray', alpha=0.8)
        plt.ylabel('PRC latency (ms)')
        plt.ylim(0, 600)
        # plt.title('Latency per number of database records')

    lgn_store = plt.legend(handles=lns,loc='upper left', ncol=1, bbox_to_anchor=(0.33*i, 1),title=str(pow(2,i)*20)+"ms delay",
    labelspacing=0.3,
    handlelength=1.0,
    handletextpad=0.4,
    borderpad=0.3,
    borderaxespad=0.2)
    lgnds.append(lgn_store)
    for l in lgnds:
        plt.gca().add_artist(l)

plt.tight_layout()
plt.savefig("latency.pdf", bbox_inches='tight')
# plt.show()

# put all the datasets into one, we don't care about network delay anymore
comb_data=data[0]+data[1]+data[2]

fig, ax1 = plt.subplots(figsize=(XSIZE, YSIZE)) # make sure everything renders the same size

# plot data y: comp time and communication size, x: records for each combination of number servers and network delay
comp = get_val_dict(comb_data,5)

for (servers,plot_data) in comp.items():
    y = [plot_data[rec] for rec in x]

    ax1.plot(x,y,color=color_dict2[str(servers)],marker=".",label="n="+str(servers))
    ax1.set_ylabel('Computation per party (ms)')    
    ax1.legend(loc='upper left',
    labelspacing=0.3,
    handlelength=1.0,
    handletextpad=0.4,
    borderpad=0.3,
    borderaxespad=0.2)
    


ax2 = ax1.twinx()

comm = get_val_dict(comb_data,6)

for (servers,plot_data) in comm.items():
    y = [plot_data[rec] for rec in x]
    ax2.plot(x,y,color=color_dict3[str(servers)],marker=".",label="n="+str(servers))
    ax2.legend(loc='upper left',
    labelspacing=0.3,
    handlelength=1.0,
    handletextpad=0.4,
    borderpad=0.3,
    borderaxespad=0.2)

    ax2.set_ylabel('Communication (KiB)')

plt.xscale('log',base=2)
plt.gca().xaxis.set_major_locator(LogLocator(base=2, numticks=10))
ax1.set_xlabel('Number of records (N)')
ax1.legend(title='Comp.',loc="upper left")
ax2.legend(title='Comm',loc="upper left",bbox_to_anchor=(0.33, 1.0))
# plt.title('Efficiency per number of database records')
plt.tight_layout()
plt.savefig("comp+comm.pdf", bbox_inches='tight')
# plt.show()

plt.figure(figsize=(XSIZE, YSIZE)) # make sure everything renders the same size
monee = get_val_dict(comb_data,42)

for (servers,plot_data) in monee.items():
    y = [plot_data[rec] for rec in x]

    plt.plot(x,y,color=color_dict4[str(servers)],marker=".",label="n="+str(servers))
    plt.axhline(y=2.5, color='gray', linestyle='--', linewidth=1)
    


    plt.xscale('log',base=2)
    plt.gca().xaxis.set_major_locator(LogLocator(base=2, numticks=10))
    plt.xlabel('Number of records (N)')
    # plt.grid(True, which='both', linestyle='-', linewidth=0.5, color='gray', alpha=0.8)
    plt.ylabel( r'Cost of \emph{1 Million} queries (\$)' if SUPPORT_TEX else  r'Cost of 1 Million queries ($)')
    # plt.title('PRC cost based on AWS pricing')
    plt.legend(loc='upper left',title="Costs")

plt.tight_layout()
plt.savefig("costs.pdf", bbox_inches='tight')
