# Redpiler

At a high level, Redpiler transforms an input Minecraft world into a directed weighted graph. The weights represent the distance taken by a redstone signal when travelling through a wire. Since Redpiler was inspired by the design of modern compilers such as LLVM, it has several passes which are run depending on how Redpiler was configured. Some passes are mandatory and are required to be run in order for redstone to function properly. Other passes are simply for optimazation and are completely optional.

Once the graph has been created, it is sent to a Redpiler backend which is responsible for the runtime execution of the Redstone circuit. There are several types of backends, but the one which is in use today is known as the Direct backend. The backend implements several optimizations such as small buffer optimization which not only reduces allocations, but also helps with memory fragmentation that can lead to cache misses. Another optimization is one focused on keeping the node list in memory as small as possible to allow the graph to fit into the small CPU caches.

# The Pass System

Redpiler was inspired by the design of modern compilers such as LLVM. As such, Redpiler has several passes which are run depending on how Redpiler was configured. Passes receive mutable access to the compile graph.

## The `IdentifyNodes` Pass

At the start of the compile, the graph is completely empty. This mandatory pass populates the graph with nodes using the given input world. This input is usually the plot the player is in, but it can also be a WorldEdit selection if Redpiler was invoked with certain flags.

The pass iterates through all the blocks in the input, and tries to identify them as Redstone components. If a block is a Repeater, Comparator, Torch, Stone Button, Lamp, Lever, Stone Pressure Plate, a new node is created in the graph with the appropriate node type containing the necessary state information. If an optimization flag is not set, Redstone Wires are also added to the graph.

Blocks that have a comparator override such as Barrels, Furnaces, Hoppers, Cauldron, Composters, and Cake are also added into the graph as constant nodes.

## The `InputSearch` Pass

Now that the graph been populated with nodes, Redpiler can now start finding the connections between Redstone components. This mandatory pass populates the graph with links.

This pass is the most complex out of them all, but it is also one of the most important aspects of Redpiler. One of the major reasons vanilla Minecraft is so slow at processing redstone is because of  the fact that when a component is updated, which can happen several times in a tick, it has to look for all sources of input. This can take a relatively very long time, and Redpiler solves this issue by only looking once and saving this information in the form of the links in the graph.

To start, this pass iterates through all nodes in the graph. Different types of nodes need to be handled differently. For example, Torches need to look for links on the block it is placed on, but Comparators and Repeaters need to look for links in the direction it is facing as well as links from the side.

When the input block of a node is searched, the block is either a component that can provide Redstone power on its own, or a Redstone Wire. If it can provide power, then it can directly create a link to that component. The corresponding node in the graph is looked up based on the position of the component, and a link to the node is created with a weight of 0. If the block is a Redstone Wire, then a breadth-first search is run to look for components that provide power to the Wire. The distance of the path taken from the starting wire to the input components are recorded as the weight of the links. Then, input components are looked up in the graph, and links are created.

## The `ClampWeights` Pass

The links created in the `InputSearch` pass are weighted by the distance taken in the breadth-first search, but this may search Wires infinetely even though wires can only have a maximum 15 signal strength that decays every block. Therefore, this optimization pass was created to remove any links with a 15 or greater weight since they ultimately have no effect.

## The `DedupLinks` Pass

Sometimes, the breadth-first search done by the `InputSearch` pass can result in two different paths to the same node. While this would not cause any problems during execution, it is still inefficent. This optimization pass removes duplicate links to the same node, only keeping the link with the lowest weight. For example, if two nodes are connected with two links of weights 13 and 15, the link with weight 15 is removed.

# The [`AnalogRepeaters`] Pass

This pass optimizes all instances of "analog repeaters", by replacing them with an equivalent comparator.
An analog repeater is a comparator, that is only connected to exactly 15 repeaters each with distances 0 counting to 14,
and then merging into only one comparator, each with again distances 0 counting to 14.

## The `ConstantFold` Pass

While nodes that are never updated in theory have no affect on the number of instructions that are run at runtime, therefore the time taken to perform a tick at runtime, keeping the size of the graph small helps to avoid cache misses that to end up taking time at runtime. This optimization pass reduces the size of the final graph by recognizing situations where a node only has constant inputs and tranforming that node into a constant node, breaking the links to the other constant nodes.

## The `UnreachableOutput` Pass

If the side of a Comparator in subtract mode is constant, then the maximum output of the comparator is equal to the difference of the maximum side input and the maximum default input. Outgoing links that have a weight greater than or equal to the maxium output of the comparator can be safely removed.

This optimization implements a simplified version of this idea. First, it iterates through all comparators in subtract mode and checks if a comparator has a single constant side input. If it does, it takes the difference between 15 and the constant strength, clamped at 0. If there are any outgoing links that have a weight greater than or equal to the difference, then it is removed from the graph.

## The `ConstantCoalesce` Pass

Disregarding High-Signal Strength logic, which Redpiler does not support anyways, the value of a constant is ever only in between 0 and 15. Effectively, there are only 16 different constant values possible. This optimization pass creates the 16 different constant nodes for all values, and removes all other constant nodes in the graph. The outgoing edges of the old constant nodes are transformed to source from the new constant nodes.

## The `Coalesce` Pass

There are often times when a wire powers many different components in the same way. For example, it is common for vertical multi-bit latches to be controlled by a slab tower that powers several repetears that lock other repeaters. This is very inefficent because these repeaters will always have the exact same value, but they are still updated and ticked independently. To avoid this logic duplication, this optimization pass merges duplicate nodes into one, removing duplicate nodes from the graph and adjusting links to point to the new node.

## The `PruneOrphans` Pass

Any redstone components that do not contribute to the functioning of output components (Trapdoors and Lamps) can be disregarded.
This pass recusively marks all nodes connected to an output node and removes all remaining unmarked nodes (Depth-First-Search).

## The `ExportGraph` Pass

This pass is neither a mandatory pass nor an optimization pass. This pass is only run when the `--export` flag is set and serializes the graph into a binary file which can be read by other programs. This can be greatly useful for people who wish to experiement with Redstone and might want a directed weighted graph just like what Redpiler creates. Using this pass, they can utilize Redpiler for their projects.

# The Backend

Once the graph has been created, it is sent to a Redpiler backend which is responsible for the runtime execution of the Redstone circuit. A backend may implement redstone executation in any way, whether that is by just-in-time compiling redstone or by interpreting the graph.

## How Redstone Works

Implementing a Redstone executor is not as trivial as it may initially seem because Redstone has very specific timing rules.

There is a difference between a Redstone component being updated and being ticked. When a redstone component is updated, it might calculate what it's state should be, and if that state is different from what its current state is, it may schedule a block tick for this component at some delay and priority. When the component is ticked, the state may change. There are 4 tick priorities which Redpiler names `Normal`, `High`, `Higher`, and `Highest`. Others may have different names for these priorities, or use number instead completely avoiding names, but it is all the same.

For example, when a Torch that is off (being powered) is updated, and it finds that it is no longer being powered and there is no tick already scheduled at this node, it will schedule a tick with delay 1 and priority `Normal`. When the time has finally come for the node to be ticked, the torch checks once again to see if it is no longer being powered, and if that is true, it sets the output strength of the node to 15 and updates all nodes that may be affected by this change.

Different node types operates differently:

### Repeater

When a Repeater is updated, the first thing that is checked is if the Repeater should be locked. If that value is different from the current state, the locking state of the Repeater is changed. Since this state change happens during the update, Repeater locking is instant. Then, if the Repeater is not locked and there is not already a tick pending at its node, then whether or not it should be powered is calculated. If this value is different from the current state, a tick is scheduled with the delay of the specific Repeater. The priority of the tick depends on if the output of the Repeater is directly facing another Repeater or Comparator. If it is, the priority is `Highest`. If not, but the Repeater is depowering, the priority is `Higher`. Otherwise, when the repeater should be powered and is not facing a Repeater or Comparator, the priority is `High`.

When a Repeater is ticked, the first thing that is checked is if the Repeater is locked. If not, then the Repeater checked if it should be powered. If the Repeater is not powered, its state is set to powered. If the Repeater is powered but should not be powered, its state is set to unpowered. Note that a Repeater will become powered here regardless of the input it is receiving, but the same is not true for depowering. If its state is changed, all nodes that may be affected by this change are updated. If the Repeater was just set to powered even though it is not receiving a non-zero input, a tick is scheduled with `Higher` priority with the delay of the Repeater. This is the only node type where a tick is scheduled in the tick function itself.

### Comparator

When a comparator is updated and there is not already a tick pending at its node, it first checks if the comparator has a far override (reading a container through a block). If it does, and regular input to the comparator is not 15, the calculated input to the Comparator is set to the value of the far override. Next, the output strength of the comparator is calculated. If the Comparator is in compare mode and the input strength is greater than or equal to the side input strength, then its output is the input strength, otherwise it is 0. If the Comparator is in subtract mode, then its output strength is its input strength subtracted by its side input strength. This obviously cannot be negative, so it is clamped at 0. Now that the output strength has been calculated, the Comparator checks if that value is different from the current state. If it is, then a tick is scheduled with a delay of 1. The priority of the tick depends on if the output of the Comparator is directly facing another Repeater or Comparator. If it is, the priority is `High`, but if not, the priority is `Normal`.

When a Comparator is ticked, the same far override check is performed, and the new output strength is calculated. If that value is different from the current state, the state of the Comparator is changed and any nodes that may be affected by this change is updated.

### Torch

When a Torch is updated and there is not already a tick pending at its node, it checks if the Torch should be off. If that value is different from the current state, then a tick is scheduled with delay 1 and priority `Normal`.

When a Torch is ticked, it checks if the Torch should be off. If that value is different from the current state, the state of the Torch is changed and any nodes that may be affected by this change is updated.

### Lamp

When a Lamp is updated, it checks if the Lamp should be lit. If a Lamp should be lit but currently is not, then the Lamp state is changed (this is instant). If the Lamp should *not* be lit, but currently is, then a tick is scheduled with delay 2 and priority `Normal`.

When a Lamp is ticked, it checks if the Lamp should be lit. If it should not be bit, but currently is, then the state of the Lamp is changed.

### Trapdoor

When a trapdoor is updated, it checks if it should be powered. If that value is different from its current state, its state is changed (this is instant).

A tick is never scheduled at a Trapdoor node, therefore a Trapdoor is never ticked.

### Wire

If a wire is updated (wire nodes only exist if unoptimized), its signal strength is calculated. If that value is different from its current state, its state is changed (this is instant). Since Wires are leaf nodes, there is no need to update any nodes here since no nodes can be affected by this change.

A tick is never scheduled at a Wire node, therefore a Wire is never ticked.

## Button

When a button is pressed and it is not powered, its state is changed to powered and any nodes that may be affected by this change is updated.

When a Button is ticked and it is currently powered, its state is changed to unpowered and any nodes that may be affected by this change is updated.

Buttons can never be updated by other nodes.

### Lever

When a lever is flicked, its state is changed to the opposite of its previous state, and any nodes that may be affected by this change is updated (this is instant).

Levers can never be updated nor ticked.

## The Direct Backend

There are several types of backends, but the one which is in use today is known as the [Direct backend](https://github.com/MCHPR/MCHPRS/tree/master/crates/core/src/redpiler/backend/direct). While this backend does not have a JIT compiler, it does implement several optimizations when compared to vanilla:

- Small buffer optimization (SBO) - This not only reduces allocations, but it also helps with memory fragmentation that can lead to cache misses.
- Node sizes are kept as small as possible in memory to allow the node list to fit into small CPU caches.
- Bounds are checked beforehand to avoid performance loss at runtime.
- The tick scheduler is powered by a rotating queue of queues that take into account that there are only 4 possible tick priorities.
