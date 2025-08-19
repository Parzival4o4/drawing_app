import menuApi from './menuApi.js';
import { LinkedListMap } from './LinkedListMap.js';


// TODO add the user id to the event id generation
// TODO create ws connection to backend 
// TODO if ws connection fails stop working and retry connection
// TODO register for the canvas (with get state option)
// TODO send all events to server 
// TODO recive events from server 

// TODO unregister when switching the page (dont forget the go back navigation of the browser)

// TODO implement user premission controll 
// TODO implement moderation 

const canvasWidth = 1024, canvasHeight = 768;
const selectorRectSize = 10;


interface ShapeFactory {
    label: string;
    handleMouseDown(x: number, y: number);
    handleMouseUp(x: number, y: number);
    handleMouseMove(x: number, y: number);
}

class Point2D {
    constructor(readonly x: number, readonly y: number) {}
}

interface Shape {
    readonly id: number;
    draw(ctx: CanvasRenderingContext2D, selected: boolean, color: string): void;
    pointInShape(point: Point2D): boolean;
    withBorderColor(color: string): Shape;
    withBackgroundColor(color: string | null): Shape;
    doStyle(ctx: CanvasRenderingContext2D);
    equals(other: Shape): boolean;
    withMove(x: number, y:number): Shape;
}

// i did a loot of thinking how to keep the Shapes as functional objects 
// i have found no other way then to add a copy with style function to each implementation 
// only the child classes now what data exists and needs to copied

// the reason i want functional shapes is that it makes event sourcing a loot easier 
// only 3 types of events: add shape, remove shape, replace shape (keeps privios z order) 
abstract class AbstractShape {
    private static counter: number = 0;
    readonly id: number;
    readonly borderColor: string;
    readonly backgroundColor: string | null;

    constructor(
        borderColor: string = 'black',
        backgroundColor: string | null = null,
        id: number | null = null // i need a way to create a shape with the same in event data 
    ) {
        if (id === null){
            this.id = AbstractShape.counter++;
        } else {
            if (id >= AbstractShape.counter){
                AbstractShape.counter = id + 1;
            }
            this.id = id
        }
        this.borderColor = borderColor;
        this.backgroundColor = backgroundColor;
    }

    abstract copyWithStyle(...styleArgs: ConstructorParameters<typeof AbstractShape>): Shape; // to implement by child classes

    withBorderColor(color: string): Shape{
        return this.copyWithStyle(color, this.backgroundColor);
    }

    withBackgroundColor(color: string | null): Shape{
        return this.copyWithStyle(this.borderColor, color);
    }

    // Draw selector rectangle
    drawSelectRect(ctx: CanvasRenderingContext2D, point: Point2D, color: string): void {
        const selectFrom = this.validPoint(
            point.x - selectorRectSize / 2,
            point.y - selectorRectSize / 2
        );

        const selectTo = this.validPoint(
            point.x + selectorRectSize / 2,
            point.y + selectorRectSize / 2
        );

        ctx.strokeStyle = color;
        ctx.beginPath();
        ctx.strokeRect( selectFrom.x, selectFrom.y, selectTo.x - selectFrom.x, selectTo.y - selectFrom.y);
        ctx.stroke();
    }

    validPoint(x: number, y: number): Point2D {
        if (x < 0) {
            x = 0;
        } else if (x > canvasWidth) {
            x = canvasWidth;
        }

        if (y < 0) {
            y = 0;
        } else if (y > canvasHeight) {
            y = canvasHeight;
        }

        return new Point2D(x, y);
    }

    doStyle(ctx: CanvasRenderingContext2D) {
        // Set the border color
        ctx.strokeStyle = this.borderColor;
        
        // Set the fill color if available, otherwise use transparent
        if (this.backgroundColor) {
            ctx.fillStyle = this.backgroundColor;
        } else {
            ctx.fillStyle = 'transparent';
        }
    }


    equals(other: Shape): boolean {
        return this.id === other.id;
    }


    withMove(dx: number, dy: number): Shape {
        return this.copyWithMove(dx, dy, this.borderColor, this.backgroundColor);
    }

    protected abstract copyWithMove(
        dx: number,
        dy: number,
        ...styleArgs: ConstructorParameters<typeof AbstractShape>
    ): Shape;
}

abstract class AbstractFactory<T extends Shape> {
    private from: Point2D;
    private tmpTo: Point2D;
    private tmpShape: T;

    constructor(readonly shapeManager: ShapeManager) {}

    abstract createShape(from: Point2D, to: Point2D): T;

    handleMouseDown(x: number, y: number) {
        this.from = new Point2D(x, y);
    }

    handleMouseUp(x: number, y: number) {
        // remove the temp line, if there was one
        if (this.tmpShape) {
            this.shapeManager.removeShapeWithId(this.tmpShape.id, false);
        }
        this.shapeManager.addShape(this.createShape(this.from, new Point2D(x,y)));
        this.from = undefined;

    }

    handleMouseMove(x: number, y: number) {
        // show temp circle only, if the start point is defined;
        if (!this.from) {
            return;
        }
        if (!this.tmpTo || (this.tmpTo.x !== x || this.tmpTo.y !== y)) {
            this.tmpTo = new Point2D(x,y);
            if (this.tmpShape) {
                // remove the old temp line, if there was one
                this.shapeManager.removeShapeWithId(this.tmpShape.id, false);
            }
            // adds a new temp line
            this.tmpShape = this.createShape(this.from, new Point2D(x,y));
            this.shapeManager.addShape(this.tmpShape);
        }
    }
}

class Line extends AbstractShape implements Shape {
    constructor(
        readonly start: Point2D,
        readonly end: Point2D,
        ...styleArgs: ConstructorParameters<typeof AbstractShape>
    ){
        super(...styleArgs);
    }


    protected copyWithMove(
        dx: number,
        dy: number,
        ...styleArgs: ConstructorParameters<typeof AbstractShape>
    ): Shape{
        const newStart = new Point2D(this.start.x + dx, this.start.y + dy);
        const newEnd = new Point2D(this.end.x + dx, this.end.y + dy);
        return new Line(newStart, newEnd, ...styleArgs);
    }



    copyWithStyle(...styleArgs: ConstructorParameters<typeof AbstractShape>): Shape {
        return new Line(this.start, this.end, ...styleArgs)
    }

    draw(ctx: CanvasRenderingContext2D, selected: boolean, color: string) {
        super.doStyle(ctx);
        ctx.beginPath();
        ctx.moveTo(this.start.x, this.start.y);
        ctx.lineTo(this.end.x, this.end.y);
        ctx.stroke();

        if (selected){
            this.drawSelectRect(ctx, this.start, color);
            this.drawSelectRect(ctx, this.end, color);
        }
    }

    //Generated by ChatGPT
    pointInShape(p: Point2D, margin: number = 5): boolean {
        // Calculate the perpendicular distance from the point to the line
        const dist = this.pointToLineDistance(p);

        // Check if the point is within the margin of the line
        if (dist <= margin) {
            // Check if the point is within the bounds of the line segment
            if (this.isPointOnLineSegment(p)) {
                return true;
            }
        }

        return false;
    }

    //Generated by ChatGPT
    private pointToLineDistance(p: Point2D): number {
        // Calculate the distance from point p to the line defined by 'start' and 'end'
        const numerator = Math.abs((this.end.y - this.start.y) * p.x - (this.end.x - this.start.x) * p.y + this.end.x * this.start.y - this.end.y * this.start.x);
        const denominator = Math.sqrt(Math.pow(this.end.y - this.start.y, 2) + Math.pow(this.end.x - this.start.x, 2));
        if (denominator === 0) return 0;
        return numerator / denominator;
    }

    //Generated by ChatGPT
    private isPointOnLineSegment(p: Point2D): boolean {
        // Check if the point p is within the bounds of the line segment (start, to)
        const minX = Math.min(this.start.x, this.end.x);
        const maxX = Math.max(this.start.x, this.end.x);
        const minY = Math.min(this.start.y, this.end.y);
        const maxY = Math.max(this.start.y, this.end.y);

        return p.x >= minX && p.x <= maxX && p.y >= minY && p.y <= maxY;
    }

}

class LineFactory extends  AbstractFactory<Line> implements ShapeFactory {

    public label: string = "Linie";

    constructor(shapeManager: ShapeManager){
        super(shapeManager);
    }

    createShape(from: Point2D, to: Point2D): Line {
        return new Line(from, to);
    }

}

class Circle extends AbstractShape implements Shape {
    constructor(
        readonly center: Point2D,
        readonly radius: number,
        ...styleArgs: ConstructorParameters<typeof AbstractShape>
    ){
        super(...styleArgs);
    }

    protected copyWithMove(
        dx: number,
        dy: number,
        ...styleArgs: ConstructorParameters<typeof AbstractShape>
    ): Shape {
        const newCenter = new Point2D(this.center.x + dx, this.center.y + dy);
        return new Circle(newCenter, this.radius, ...styleArgs);
    }


    copyWithStyle(...styleArgs: ConstructorParameters<typeof AbstractShape>): Shape {
        return new Circle(this.center, this.radius, ...styleArgs)
    }

    draw(ctx: CanvasRenderingContext2D, selected: boolean, color: string) {
        super.doStyle(ctx);
        ctx.beginPath();
        ctx.arc(this.center.x,this.center.y,this.radius,0,2*Math.PI);
        ctx.fill();
        ctx.stroke();

        if (selected){
            this.drawSelectRect(ctx, this.center, color);
        }
    }

    //Generated by ChatGPT
    pointInShape(p: Point2D): boolean {
        const dx = p.x - this.center.x;
        const dy = p.y - this.center.y;
        const distanceSquared = dx * dx + dy * dy;
        return distanceSquared <= this.radius * this.radius;
    }
}

class CircleFactory extends AbstractFactory<Circle> implements ShapeFactory{
    public label: string = "Kreis";

    constructor(shapeManager: ShapeManager){
        super(shapeManager);
    }

    createShape(from: Point2D, to: Point2D): Circle {
        return new Circle(from, CircleFactory.computeRadius(from, to.x, to.y));
    }

    private static computeRadius(from: Point2D, x: number, y: number): number {
        const xDiff = (from.x - x),
            yDiff = (from.y - y);
        return Math.sqrt(xDiff * xDiff + yDiff * yDiff);
    }
}

class Rectangle extends AbstractShape implements Shape {
    constructor(
        readonly from: Point2D,
        readonly to: Point2D,
        ...styleArgs: ConstructorParameters<typeof AbstractShape>
    ){
        super(...styleArgs);
    }

    protected copyWithMove(
        dx: number,
        dy: number,
        ...styleArgs: ConstructorParameters<typeof AbstractShape>
    ): Shape {
        const newFrom = new Point2D(this.from.x + dx, this.from.y + dy);
        const newTo = new Point2D(this.to.x + dx, this.to.y + dy);
        return new Rectangle(newFrom, newTo, ...styleArgs);
    }

    copyWithStyle(...styleArgs: ConstructorParameters<typeof AbstractShape>): Shape {
        return new Rectangle(this.from, this.to, ...styleArgs)
    }

    draw(ctx: CanvasRenderingContext2D, selected: boolean, color: string) {
        super.doStyle(ctx);
        ctx.beginPath();
        ctx.fillRect(this.from.x, this.from.y,
            this.to.x - this.from.x, this.to.y - this.from.y);
        ctx.strokeRect(this.from.x, this.from.y,
            this.to.x - this.from.x, this.to.y - this.from.y);
        ctx.stroke();

        if (selected){
            this.drawSelectRect(ctx, this.from, color);
            this.drawSelectRect(ctx, this.to, color);
            this.drawSelectRect(ctx, new Point2D(this.from.x, this.to.y) , color);
            this.drawSelectRect(ctx, new Point2D(this.to.x, this.from.y) , color);
        }
    }

    //Generated by ChatGPT
    pointInShape(p: Point2D): boolean {
        // Check if the point (p) is within the bounds of the rectangle
        const minX = Math.min(this.from.x, this.to.x);
        const maxX = Math.max(this.from.x, this.to.x);
        const minY = Math.min(this.from.y, this.to.y);
        const maxY = Math.max(this.from.y, this.to.y);
        return p.x >= minX && p.x <= maxX && p.y >= minY && p.y <= maxY;
    }
}

class RectangleFactory extends AbstractFactory<Rectangle> implements ShapeFactory{
    public label: string = "Rechteck";
    constructor(shapeManager: ShapeManager){
        super(shapeManager);
    }

    createShape(from: Point2D, to: Point2D): Rectangle {
        return new Rectangle(from, to);
    }
}

class Triangle extends AbstractShape implements Shape {

    constructor(
        readonly p1: Point2D,
        readonly p2: Point2D,
        readonly p3: Point2D,
        ...styleArgs: ConstructorParameters<typeof AbstractShape>
    ){
        super(...styleArgs);
    }

    protected copyWithMove(
        dx: number,
        dy: number,
        ...styleArgs: ConstructorParameters<typeof AbstractShape>
    ): Shape {
        const newP1 = new Point2D(this.p1.x + dx, this.p1.y + dy);
        const newP2 = new Point2D(this.p2.x + dx, this.p2.y + dy);
        const newP3 = new Point2D(this.p3.x + dx, this.p3.y + dy);
        return new Triangle(newP1, newP2, newP3, ...styleArgs);
    }

    copyWithStyle(...styleArgs: ConstructorParameters<typeof AbstractShape>): Shape {
        return new Triangle(this.p1, this.p2, this.p3, ...styleArgs)
    }

    draw(ctx: CanvasRenderingContext2D, selected: boolean, color: string) {
        super.doStyle(ctx);
        ctx.beginPath();
        ctx.moveTo(this.p1.x, this.p1.y);
        ctx.lineTo(this.p2.x, this.p2.y);
        ctx.lineTo(this.p3.x, this.p3.y);
        ctx.lineTo(this.p1.x, this.p1.y);
        ctx.fill();
        ctx.stroke();

        if (selected){
            this.drawSelectRect(ctx, this.p1, color);
            this.drawSelectRect(ctx, this.p2, color);
            this.drawSelectRect(ctx, this.p3, color);
        }
    }

    //Generated by ChatGPT
    pointInShape(p: Point2D): boolean {
        // Area of the full triangle
        const areaOrig = this.triangleArea(this.p1, this.p2, this.p3);

        // Areas of the sub-triangles formed with the point
        const area1 = this.triangleArea(p, this.p2, this.p3);
        const area2 = this.triangleArea(this.p1, p, this.p3);
        const area3 = this.triangleArea(this.p1, this.p2, p);

        // If the sum of the areas of the sub-triangles is equal to the area of the original triangle, the point is inside
        return Math.abs(areaOrig - (area1 + area2 + area3)) < 0.01;
    }

    //Generated by ChatGPT
    // Helper function to calculate the area of a triangle using the determinant method
    private triangleArea(p1: Point2D, p2: Point2D, p3: Point2D): number {
        return Math.abs((p1.x * (p2.y - p3.y) + p2.x * (p3.y - p1.y) + p3.x * (p1.y - p2.y)) / 2);
    }
}

class TriangleFactory implements ShapeFactory{
    public label: string = "Dreieck";

    private from: Point2D;
    private tmpTo: Point2D;
    private tmpLine: Line;
    private thirdPoint: Point2D;
    private tmpShape: Triangle;

    constructor(readonly shapeManager: ShapeManager) {}

    handleMouseDown(x: number, y: number) {
        if (this.tmpShape) {
            this.shapeManager.removeShapeWithId(this.tmpShape.id, false);
            this.shapeManager.addShape(
                new Triangle(this.from, this.tmpTo, new Point2D(x,y)));
            this.from = undefined;
            this.tmpTo = undefined;
            this.tmpLine = undefined;
            this.thirdPoint = undefined;
            this.tmpShape = undefined;
        } else {
            this.from = new Point2D(x, y);
        }
    }

    handleMouseUp(x: number, y: number) {
        // remove the temp line, if there was one
        if (this.tmpLine) {
            this.shapeManager.removeShapeWithId(this.tmpLine.id, false);
            this.tmpLine = undefined;
            this.tmpTo = new Point2D(x,y);
            this.thirdPoint = new Point2D(x,y);
            this.tmpShape = new Triangle(this.from, this.tmpTo, this.thirdPoint);
            this.shapeManager.addShape(this.tmpShape);
        }
    }

    handleMouseMove(x: number, y: number) {
        // show temp circle only, if the start point is defined;
        if (!this.from) {
            return;
        }

        if (this.tmpShape) { // second point already defined, update temp triangle
            if (!this.thirdPoint || (this.thirdPoint.x !== x || this.thirdPoint.y !== y)) {
                this.thirdPoint = new Point2D(x,y);
                if (this.tmpShape) {
                    // remove the old temp line, if there was one
                    this.shapeManager.removeShapeWithId(this.tmpShape.id, false);
                }
                // adds a new temp triangle
                this.tmpShape = new Triangle(this.from, this.tmpTo, this.thirdPoint);
                this.shapeManager.addShape(this.tmpShape);
            }
        } else { // no second point fixed, update tmp line
            if (!this.tmpTo || (this.tmpTo.x !== x || this.tmpTo.y !== y)) {
                this.tmpTo = new Point2D(x,y);
                if (this.tmpLine) {
                    // remove the old temp line, if there was one
                    this.shapeManager.removeShapeWithId(this.tmpLine.id, false);
                }
                // adds a new temp line
                this.tmpLine = new Line(this.from, this.tmpTo);
                this.shapeManager.addShape(this.tmpLine);
            }
        }
    }
}

// class Shapes {
// }




class SelectTool implements ShapeFactory {
    label: string = "Select";

    // State for Alt-based cycling
    private altClickPoint: Point2D | null = null;
    private altIndex: number = -1;

    // Modifier flags
    private altPressed: boolean = false;
    private ctrlPressed: boolean = false;

    // DragDrop
    private dragPoint: Point2D | null = null;
    private dragged: boolean = false;

    constructor(readonly shapeManager: ShapeManager) {
        // Track Alt and Ctrl key state
        window.addEventListener("keydown", (e: KeyboardEvent) => {
            if (e.key === "Alt") this.altPressed = true;
            if (e.key === "Control") this.ctrlPressed = true;
        });
        window.addEventListener("keyup", (e: KeyboardEvent) => {
            if (e.key === "Alt") {
                this.altPressed = false;

                this.altClickPoint= null; 
                this.altIndex = -1;
            }
            if (e.key === "Control") this.ctrlPressed = false;
        });
    }

    handleMouseDown(x: number, y: number) {
        this.dragPoint = new Point2D(x, y);
    }

    handleMouseUp(x: number, y: number) {
        if(!this.dragged){
            // if there was no movement it was a selection click
            let index = 0; 
            let ids = [];
            let additive = this.ctrlPressed;

            if(this.altPressed){
                if (!this.altClickPoint){
                    this.altClickPoint = new Point2D(x,y);
                    this.altIndex = -1;
                } 

                ids = this.shapeManager.getShapeIdsAtPoint(this.altClickPoint.x, this.altClickPoint.y);

                if (ids.length > 0 ){
                    this.altIndex = (this.altIndex + 1) % ids.length;
                    index = this.altIndex;
                } 
            } else {
                ids = this.shapeManager.getShapeIdsAtPoint(x, y);
            }

            if (ids.length > index){
                this.shapeManager.selectShapeById(ids[index], additive);
            } else {
                // deselect all 
                this.shapeManager.selectShapeById(-1, false);
            }
        }
        this.dragPoint= null;
        this.dragged = false;
    }

    handleMouseMove(x: number, y: number) {
        if (this.dragPoint != null) {
            this.dragged = true;
            const ids = this.shapeManager.getSelectedIds();
            if (ids.length === 0) {
                // Don't even need to update the dragPoint, nothing is selected
                return;
            }

            const dx = x - this.dragPoint.x;
            const dy = y - this.dragPoint.y;

            ids.forEach((id, index) => {
                const shape = this.shapeManager.getShapeWithId(id);
                const newShape = shape.withMove(dx, dy);
                const isLast = index === ids.length - 1;
                this.shapeManager.replaceShape(id, newShape, isLast); // redraw only on the last one
            });

            // Update dragPoint to avoid accumulating delta
            this.dragPoint = new Point2D(x, y);
        }
    }

}



class ToolArea {
    private selectedShape: ShapeFactory = undefined;
    constructor(shapesSelector: ShapeFactory[], menue: Element) {
        const domElms = [];
        shapesSelector.forEach(sl => {
            const domSelElement = document.createElement("li");
            domSelElement.innerText = sl.label;
            menue.appendChild(domSelElement);
            domElms.push(domSelElement);

            domSelElement.addEventListener("click", () => {
                selectFactory.call(this, sl, domSelElement);
            });
        });

        function selectFactory(sl: ShapeFactory, domElm: HTMLElement) {
            // remove class from all elements
            for (let j = 0; j < domElms.length; j++) {
                domElms[j].classList.remove("marked");
            }
            this.selectedShape = sl;
            // add class to the one that is selected currently
            domElm.classList.add("marked");
        }
    }

    getSelectedShape(): ShapeFactory {
        return this.selectedShape;
    }

}

interface ShapeManager {
    // stuff that changes things 
    addShape(shape: Shape, redraw?: boolean): this;
    removeShapeWithId(id: number, redraw?: boolean): this;
    selectShapeById(id: number, additive: boolean): void;
    bringSelectedToFront(redraw?: boolean): this;
    sendSelectedToBack(redraw?: boolean): this;
    replaceShape(oldId: number, newShape: Shape, redraw?: boolean): this; 

    // alias 
    removeShape(shape: Shape, redraw?: boolean): this;

    // geters 
    getShapeIdsAtPoint(x: number, y: number): number[];
    getSelectedIds(): number[];
    getShapeWithId(id:number): Shape; 
}

class Canvas {
    private ctx: CanvasRenderingContext2D;
    private shapes: LinkedListMap<Shape> = new LinkedListMap();
    private selectedShapes: Set<number> = new Set();

    constructor(canvasDomElement: HTMLCanvasElement, toolarea: ToolArea) {
        this.ctx = canvasDomElement.getContext("2d")!;
        canvasDomElement.addEventListener("mousemove", createMouseHandler("handleMouseMove"));
        canvasDomElement.addEventListener("mousedown", createMouseHandler("handleMouseDown"));
        canvasDomElement.addEventListener("mouseup", createMouseHandler("handleMouseUp"));

        function createMouseHandler(methodName: string) {
            return function (e) {
                e = e || window.event;

                if ('object' === typeof e) {
                    const btnCode = e.button,
                        x = e.pageX - this.offsetLeft,
                        y = e.pageY - this.offsetTop,
                        ss = toolarea.getSelectedShape();
                    // if left mouse button is pressed,
                    // and if a tool is selected, do something
                    if (e.button === 0 && ss) {
                        const m = ss[methodName];
                        // This in the shapeFactory should be the factory itself.
                        m.call(ss, x, y);
                    }
                }
            }
        }
    }

    draw(): this {
        this.ctx.beginPath();
        this.ctx.fillStyle = 'lightgrey';
        this.ctx.fillRect(0, 0, canvasWidth, canvasHeight);
        this.ctx.stroke();

        for (const shape of this.shapes) {
            const isSelected = this.selectedShapes.has(shape.id);
            shape.draw(this.ctx, isSelected, 'red');
        }

        return this;
    }

    // dose stuff
    private addShape(shape: Shape, redraw: boolean = true): this {
        // add shape at top of zorder
        this.shapes.add(shape.id, shape);
        return redraw ? this.draw() : this;
    }

    private removeShapeWithId(id: number, redraw: boolean = true): this {
        // Remove the shape from the main shape list
        this.shapes.removeById(id);
        this.selectedShapes.delete(id);
        return redraw ? this.draw() : this;
    }

    private selectShapeById(id: number, additive: boolean = false): void {
        if (!additive) {
            this.selectedShapes.clear();
        }

        if (this.selectedShapes.has(id)) {
            this.selectedShapes.delete(id);  // Deselect if already selected
        } else if (id !== -1) {
            this.selectedShapes.add(id);
        }

        this.draw();
    }

    /**
     * Moves all selected shapes to the front (top of z-order).
     * First element in the list is back, last is front.
     */
    private bringSelectedToFront(redraw: boolean = true): this {
        // We move each selected shape in turn
        for (const id of this.selectedShapes) {
            this.shapes.moveToEnd(id);
        }
        return redraw ? this.draw() : this;
    }

    /**
     * Moves all selected shapes to the back (bottom of z-order).
     */
    private sendSelectedToBack(redraw: boolean = true): this {
        for (const id of this.selectedShapes) {
            this.shapes.moveToStart(id);
        }
        return redraw ? this.draw() : this;
    }

    replaceShape(oldId: number, newShape: Shape, redraw: boolean = true): this {
        this.shapes.replace(oldId, newShape.id, newShape)

        if (this.selectedShapes.has(oldId)) {
            this.selectedShapes.delete(oldId);
            this.selectedShapes.add(newShape.id);
        }

        return redraw ? this.draw() : this;
    }

    //alias
    private removeShape(shape: Shape, redraw: boolean = true): this {
        return this.removeShapeWithId(shape.id, redraw);
    }


    apply(event: any){
        this.instantiateShapeFromData(event);
        switch (event.type) {
            case "shapeAdded": 
                this.addShape(event.shape, event.redraw);
                break;
            case "shapeRemoved": 
                this.removeShape(event.shape, event.redraw);
                break;
            case "shapeRemovedWithId": 
                this.removeShapeWithId(event.shapeId, event.redraw);
                break;
            case "shapeReplaced":
                this.replaceShape(event.oldId, event.shape, event.redraw);
                break;
            case "selectedBroughtToFront": 
                this.bringSelectedToFront(event.redraw);
                break;
            case "selectedBroughtToBack": 
                this.sendSelectedToBack(event.redraw);
                break;
            case "shapeSelected": 
                this.selectShapeById(event.id, event.additive);
                break
        }
    }

    private instantiateShapeFromData(object: any) {
        if ("shape" in object && !object.shape.draw) {
            const s = object.shape;
            const borderColor = s.borderColor ?? "black";
            const backgroundColor = s.backgroundColor ?? null;
            const id = s.id ?? null;

            // Circle
            if ("radius" in s && "center" in s) {
                object.shape = new Circle(
                    new Point2D(s.center.x, s.center.y),
                    s.radius,
                    borderColor,
                    backgroundColor,
                    id
                );
            }

            // Line
            else if ("start" in s && "end" in s) {
                object.shape = new Line(
                    new Point2D(s.start.x, s.start.y),
                    new Point2D(s.end.x, s.end.y),
                    borderColor,
                    backgroundColor,
                    id
                );
            }

            // Rectangle
            else if ("from" in s && "to" in s) {
                object.shape = new Rectangle(
                    new Point2D(s.from.x, s.from.y),
                    new Point2D(s.to.x, s.to.y),
                    borderColor,
                    backgroundColor,
                    id
                );
            }

            // Triangle
            else if ("p1" in s && "p2" in s && "p3" in s) {
                object.shape = new Triangle(
                    new Point2D(s.p1.x, s.p1.y),
                    new Point2D(s.p2.x, s.p2.y),
                    new Point2D(s.p3.x, s.p3.y),
                    borderColor,
                    backgroundColor,
                    id
                );
            }
        }
    }

    //getter
    getShapeIdsAtPoint(x: number, y: number): number[] {
        const pt = new Point2D(x, y);
        const result: number[] = [];

        for (const shape of this.shapes) {
            if (shape.pointInShape(pt)) {
                result.push(shape.id);
            }
        }

        // Sort ascending by ID for consistency
        return result.sort((a, b) => a - b);
    }

    getSelectedIds(): number[] {
        return Array.from(this.selectedShapes);
    }

    getShapeWithId(id: number): Shape {
        const shape = this.shapes.getById(id);
        if (!shape) {
            throw new Error(`No shape with ID ${id} found.`);
        }
        return shape;
    }

    reset(): this {
        this.shapes = new LinkedListMap();
        this.selectedShapes.clear();
        return this.draw();
    }
}


class EventSystem {
    private readonly handlers = []

    public register(handler: any) {
        this.handlers.push(handler);
    }

    apply(event: any) {
        this.handlers.forEach(h => h(event));
    }
}

class EventSystemUI {
    constructor(readonly es, readonly canvas: Canvas, textAreaDomElm: HTMLTextAreaElement, buttonDomElm: HTMLButtonElement) {
        // connect textArea to event system
        let save = true;

        //registriert sich bei dem event system
        es.register((event: any) => {
            if  (save === true){
                //printet die erhaltenen events auf dem text feld aus
                textAreaDomElm.value += JSON.stringify(event) + "\n" ;
            }
        });

        // buttonDomElm.addEventListener("click", canvas.redrawFromText);
        buttonDomElm.addEventListener("click", () => {
            save = false;
            this.canvas.reset();
            let lines = textAreaDomElm.value.replace(/\r\n/g,"\n").split("\n");
            console.log("Zeilen", lines)
            const dataObject = lines.filter(l => l != "") .map(l => JSON.parse(l));
            this.readDataObject(dataObject);
            save = true;
        })
    }

    private readDataObject(objects) {
        objects.forEach(o => this.es.apply(o));
    }

}


export function setupDrawer(
  canvasDomElm: HTMLCanvasElement,
  menuElm: HTMLElement,
  textAreaDomElm: HTMLTextAreaElement,
  buttonDomElm: HTMLButtonElement
) {

  let es: EventSystem;
  let canvas: Canvas;
  const sm: ShapeManager = {
    addShape(s, rd) {
      const event = { type: "shapeAdded", shape: s, redraw: rd };
      es.apply(event);
      return this;
    },
    removeShape(s, rd) {
      es.apply({ type: "shapeRemoved", shape: s, redraw: rd });
      return this;
    },
    removeShapeWithId(id, rd) {
      es.apply({ type: "shapeRemovedWithId", shapeId: id, redraw: rd });
      return this;
    },
    replaceShape(oldId: number, newShape: Shape, redraw?: boolean) {
      es.apply({ type: "shapeReplaced", oldId, shape: newShape, redraw });
      return this;
    },
    bringSelectedToFront(redraw) {
      es.apply({ type: "selectedBroughtToFront", redraw: redraw ?? true });
      return this;
    },
    sendSelectedToBack(redraw) {
      es.apply({ type: "selectedBroughtToBack", redraw: redraw ?? true });
      return this;
    },
    selectShapeById(id, additive) {
      es.apply({ type: "shapeSelected", id, additive });
      return this;
    },
    getShapeIdsAtPoint(x, y): number[] {
      return canvas.getShapeIdsAtPoint(x, y);
    },
    getSelectedIds() {
      return canvas.getSelectedIds();
    },
    getShapeWithId(id: number): Shape {
      return canvas.getShapeWithId(id);
    },
  };

  const shapesSelector: ShapeFactory[] = [
    new LineFactory(sm),
    new CircleFactory(sm),
    new RectangleFactory(sm),
    new TriangleFactory(sm),
    new SelectTool(sm),
  ];

  const toolArea = new ToolArea(shapesSelector, menuElm);

  canvas = new Canvas(canvasDomElm, toolArea);
  canvas.draw();

  es = new EventSystem();
  es.register((event: any) => canvas.apply(event));
  const esui = new EventSystemUI(es, canvas, textAreaDomElm, buttonDomElm);

  // -------------------------
  // Popup menu integration
  // -------------------------

  const contextMenu = menuApi.createMenu();

  contextMenu.addItem(
    menuApi.createItem("Delete Selected", () => {
      const ids = sm.getSelectedIds();
      ids.forEach((id) => sm.removeShapeWithId(id, false));
      canvas.draw();
      contextMenu.hide();
    })
  );

  contextMenu.addItem(menuApi.createSeparator());

  contextMenu.addItem(
    menuApi.createRadioOption(
      "Rahmenfarbe",
      { black: "Black", red: "Red", green: "Green", yellow: "Yellow", blue: "Blue" },
      "black",
      (key) => {
        sm.getSelectedIds().forEach((id) => {
          const original = sm.getShapeWithId(id);
          const updated = original.withBorderColor(key);
          sm.replaceShape(id, updated, false);
        });
        canvas.draw();
      }
    )
  );

  contextMenu.addItem(
    menuApi.createRadioOption(
      "Hintergrundfarbe",
      { transparent: "Transparent", black: "Black", red: "Red", green: "Green", yellow: "Yellow", blue: "Blue" },
      "white",
      (key) => {
        sm.getSelectedIds().forEach((id) => {
          const original = sm.getShapeWithId(id);
          const color = key === "transparent" ? null : key;
          const updated = original.withBackgroundColor(color);
          sm.replaceShape(id, updated, false);
        });
        canvas.draw();
      }
    )
  );

  contextMenu.addItem(menuApi.createSeparator());

  contextMenu.addItem(
    menuApi.createItem("Bring to Front", () => {
      sm.bringSelectedToFront();
    })
  );

  contextMenu.addItem(
    menuApi.createItem("Send to Back", () => {
      sm.sendSelectedToBack();
    })
  );

  canvasDomElm.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    const rect = canvasDomElm.getBoundingClientRect();
    contextMenu.show(e.clientX - rect.left, e.clientY - rect.top);
  });
}
