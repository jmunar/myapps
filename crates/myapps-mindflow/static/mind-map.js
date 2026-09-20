// Force-directed mind map (d3). Reads its base path from <html data-base> and
// its empty-state label from #mindmap's data attributes, so this file is the
// same bytes in every language and needs nothing interpolated into it.

(function() {
    var basePath = document.documentElement.dataset.base || '';
    var width, height;
    var svg, simulation;
    var container = document.getElementById('mindmap');
    if (!container) return;
    var emptyLabel = container.dataset.emptyLabel || '';

    function initMap() {
        width = container.clientWidth || 800;
        height = 500;

        container.innerHTML = '';
        svg = d3.select('#mindmap')
            .append('svg')
            .attr('width', width)
            .attr('height', height)
            .attr('viewBox', [0, 0, width, height]);

        window._mapG = svg.append('g');

        // Add zoom + pan behavior
        var zoom = d3.zoom()
            .scaleExtent([0.3, 3])
            .on('zoom', function(event) {
                window._mapG.attr('transform', event.transform);
            });
        svg.call(zoom);
    }

    window.refreshMap = function() {
        fetch(basePath + '/mindflow/map-data')
            .then(function(r) { return r.json(); })
            .then(function(data) { renderGraph(data); });
    };

    function renderGraph(data) {
        var g = window._mapG;
        g.selectAll('*').remove();

        if (data.nodes.length === 0) {
            svg.append('text')
                .attr('x', width / 2)
                .attr('y', height / 2)
                .attr('text-anchor', 'middle')
                .attr('fill', 'var(--text-secondary)')
                .text(emptyLabel);
            return;
        }

        simulation = d3.forceSimulation(data.nodes)
            .force('link', d3.forceLink(data.links).id(function(d) { return d.id; }).distance(55))
            .force('charge', d3.forceManyBody().strength(-100))
            .force('center', d3.forceCenter(width / 2, height / 2))
            .force('collision', d3.forceCollide().radius(function(d) {
                return d.type === 'category' ? 35 : 22;
            }));

        var link = g.append('g')
            .selectAll('line')
            .data(data.links)
            .join('line')
            .attr('stroke', '#ccc')
            .attr('stroke-width', 1.5);

        var node = g.append('g')
            .selectAll('g')
            .data(data.nodes)
            .join('g')
            .call(d3.drag()
                .on('start', function(event, d) {
                    if (!event.active) simulation.alphaTarget(0.3).restart();
                    d.fx = d.x; d.fy = d.y;
                })
                .on('drag', function(event, d) {
                    d.fx = event.x; d.fy = event.y;
                })
                .on('end', function(event, d) {
                    if (!event.active) simulation.alphaTarget(0);
                    d.fx = null; d.fy = null;
                }));

        // Category nodes: larger circles
        node.filter(function(d) { return d.type === 'category'; })
            .append('circle')
            .attr('r', 30)
            .attr('fill', function(d) { return d.color || '#6B6B6B'; })
            .attr('opacity', 0.8)
            .attr('stroke', '#fff')
            .attr('stroke-width', 2);

        node.filter(function(d) { return d.type === 'category'; })
            .append('text')
            .text(function(d) { return d.name; })
            .attr('text-anchor', 'middle')
            .attr('dy', '0.35em')
            .attr('fill', '#fff')
            .attr('font-size', '11px')
            .attr('font-weight', '600')
            .style('pointer-events', 'none');

        // Thought nodes: smaller circles, clickable
        var thoughtNodes = node.filter(function(d) { return d.type === 'thought'; });

        thoughtNodes.append('circle')
            .attr('r', 14)
            .attr('fill', function(d) { return d.color || '#ddd'; })
            .attr('opacity', 0.9)
            .attr('stroke', '#fff')
            .attr('stroke-width', 1)
            .style('cursor', 'pointer');

        // Always-visible short labels next to thought nodes
        thoughtNodes.append('text')
            .text(function(d) {
                var s = d.name;
                return s.length > 20 ? s.substring(0, 18) + '...' : s;
            })
            .attr('dx', 18)
            .attr('dy', '0.35em')
            .attr('fill', 'var(--text-secondary)')
            .attr('font-size', '9px')
            .attr('font-family', 'var(--font-body)')
            .style('pointer-events', 'none')
            .attr('class', 'thought-label');

        // Tap-to-select on touch, click-to-navigate on desktop
        var selectedNode = null;

        thoughtNodes.on('click', function(event, d) {
            var isTouch = 'ontouchstart' in window;
            if (isTouch && selectedNode !== d.id) {
                // First tap: select and highlight
                event.stopPropagation();
                selectedNode = d.id;
                // Reset all thought circles
                thoughtNodes.select('circle')
                    .attr('stroke', '#fff')
                    .attr('stroke-width', 1)
                    .attr('r', 14);
                // Highlight selected
                d3.select(this).select('circle')
                    .attr('stroke', 'var(--accent)')
                    .attr('stroke-width', 3)
                    .attr('r', 18);
                // Show full text in tooltip
                tooltip.text(d.name)
                    .style('left', '1rem')
                    .style('bottom', '1rem')
                    .style('top', 'auto')
                    .style('opacity', 1);
            } else {
                // Second tap or desktop click: navigate
                window.location.href = basePath + '/mindflow/thoughts/' + d.thought_id;
            }
        });

        // Tap empty area to deselect on touch
        svg.on('click', function() {
            if (selectedNode) {
                selectedNode = null;
                thoughtNodes.select('circle')
                    .attr('stroke', '#fff')
                    .attr('stroke-width', 1)
                    .attr('r', 14);
                tooltip.style('opacity', 0);
            }
        });

        // Desktop tooltip (instant hover)
        var tooltip = d3.select('#mindmap')
            .append('div')
            .attr('class', 'map-tooltip')
            .style('opacity', 0);

        node.on('mouseenter', function(event, d) {
                if ('ontouchstart' in window) return;
                tooltip.text(d.name)
                    .style('left', (event.offsetX + 12) + 'px')
                    .style('top', (event.offsetY - 8) + 'px')
                    .style('bottom', 'auto')
                    .style('opacity', 1);
            })
            .on('mousemove', function(event) {
                if ('ontouchstart' in window) return;
                tooltip
                    .style('left', (event.offsetX + 12) + 'px')
                    .style('top', (event.offsetY - 8) + 'px');
            })
            .on('mouseleave', function() {
                if ('ontouchstart' in window) return;
                tooltip.style('opacity', 0);
            });

        simulation.on('tick', function() {
            link.attr('x1', function(d) { return d.source.x; })
                .attr('y1', function(d) { return d.source.y; })
                .attr('x2', function(d) { return d.target.x; })
                .attr('y2', function(d) { return d.target.y; });
            node.attr('transform', function(d) {
                return 'translate(' + d.x + ',' + d.y + ')';
            });
        });
    }

    initMap();
    refreshMap();
})();
